//! MediaRouter —— Tauri v2 实现。

pub mod commands;
pub mod error;
pub mod imagedims;
pub mod launcher;
pub mod metadata;
pub mod model;
pub mod paths;
pub mod platform;
pub mod scanner;
pub mod sort;
pub mod state;
pub mod store;
pub mod thumbs;

use std::path::Path;

use tauri::http::{Request, Response, StatusCode};
use tauri::Manager;

use state::AppState;

/// 缩略图资源协议处理器。
///
/// 只服务缩略图缓存目录里的文件，且文件名必须是 32 位小写十六进制
/// （也就是 `hash_of` 的产物），以此挡住路径穿越与任意文件读取。
fn serve_thumb(app: &tauri::AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {    let deny = |code: StatusCode| {
        Response::builder()
            .status(code)
            .body(Vec::new())
            .expect("构造空响应")
    };

    // 请求形如 http://thumb.localhost/<percent-encoded 绝对路径>
    let raw = request.uri().path().trim_start_matches('/');
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8_lossy()
        .into_owned();

    // 明确挡一层穿越（`starts_with` 之外再加一道保险）
    if decoded.contains("..") {
        return deny(StatusCode::NOT_FOUND);
    }

    let state = app.state::<AppState>();
    let thumbs_dir = state.thumbs.dir();
    let candidate = Path::new(&decoded);

    if !candidate.starts_with(thumbs_dir) || !candidate.is_file() {
        return deny(StatusCode::NOT_FOUND);
    }

    let Some(name) = candidate.file_name().and_then(|s| s.to_str()) else {
        return deny(StatusCode::NOT_FOUND);
    };
    let is_hash = name.len() == 32
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !is_hash {
        return deny(StatusCode::NOT_FOUND);
    }

    let Ok(bytes) = std::fs::read(candidate) else {
        return deny(StatusCode::NOT_FOUND);
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", thumbs::mime::sniff_mime(&bytes))
        .header("cache-control", "max-age=31536000, immutable")
        .body(bytes)
        .unwrap_or_else(|_| deny(StatusCode::INTERNAL_SERVER_ERROR))
}

/// 原图资源协议处理器（服务「图片」分类的三级菜单）。
///
/// ## 为什么这里**没有**它
///
/// 曾经实现过：直接读原文件发给渲染层。理由是"图片浏览就该看原图"，
/// 而且省掉给几千张图逐个生成缩略图的 CPU。
///
/// **这个判断是错的，而且代价很大。** 它是按"一张图"在想问题，
/// 按"一屏图"想就完全不同：
///
/// - 一张 4000×3000 的照片解码成位图是 **48MB**，一屏铺 20 张 ≈ 1GB，
///   滚动时还要不断换入换出 —— 用户反馈的"很吃内存"主要就来自这里；
/// - 而图墙的格子在 188px 行高下最多也就三五百像素宽，原图的分辨率
///   99% 是被浪费的：解码、上传、再缩小绘制。
///
/// 现在统一走 `thumb` 协议（长边 512px，磁盘缓存）。铺满格子仍是过采样，
/// 解码后约 0.6MB —— 两个数量级的差距。
///
/// 教训写在这里，免得下次又有人觉得"原图更清楚"就把它加回来。

pub fn run() {
    let app = tauri::Builder::default()
        // 重复启动时聚焦已有窗口（旧版是 requestSingleInstanceLock）
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .register_uri_scheme_protocol("thumb", |ctx, request| {
            serve_thumb(ctx.app_handle(), request)
        })
        .setup(|app| {
            let root = paths::resolve_data_dir(app.handle());
            println!("[mediarouter] 数据目录: {}", root.display());

            let state = AppState::bootstrap(root);
            println!(
                "[mediarouter] 缩略图缓存已就绪（当前 {} 张）",
                state.thumbs.count()
            );
            app.manage(state);

            // 先开窗，再后台预扫：窗口创建不被全量扫描阻塞。
            // 渲染层首次 library_get 会复用同一次扫描结果，不会重复扫。
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                let lib = state.library().await;
                println!("[mediarouter] 初始扫描完成: {}", scanner::summarize(&lib));
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::library_get,
            commands::library_rescan,
            commands::settings_get,
            commands::settings_save,
            commands::rules_get,
            commands::rules_save,
            commands::read_get,
            commands::read_toggle,
            commands::read_set,
            commands::read_clear,
            commands::file_open,
            commands::file_open_default,
            commands::file_reveal,
            commands::file_copy,
            commands::exe_validate,
            commands::dialog_pick_directory,
            commands::dialog_pick_exe,
            commands::file_meta,
            commands::meta_cache_count,
            commands::ffmpeg_status,
            commands::file_thumb,
            commands::file_save_thumb,
            commands::thumbs_clear,
            commands::thumbs_count,
            commands::read_file_data,
            commands::read_file_range,
            commands::ui_ready,
            commands::app_data_dir,
        ])
        .build(tauri::generate_context!())
        .expect("MediaRouter 启动失败");

    app.run(|handle, event| {
        // 退出前把时长缓存落盘。落盘有 2 秒节流，不补这一刀的话，
        // 「探测完就立刻关窗」的那批时长会丢。
        if let tauri::RunEvent::Exit = event {
            if let Some(state) = handle.try_state::<AppState>() {
                state.flush_meta();
                println!("[mediarouter] 退出：时长缓存已落盘");
            }
        }
    });
}
