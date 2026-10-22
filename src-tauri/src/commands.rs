//! 前端可调用的全部命令。
//!
//! 这一层对应旧版的 `electron/ipc.js` + `preload.js`：
//! 旧版是「ipcMain.handle 注册通道 + contextBridge 暴露 window.api」两步，
//! Tauri 里合成一步 —— `#[tauri::command]` 就是通道，前端用 `invoke()` 直接调。

use std::path::{Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::error::{AppError, AppResult};
use crate::launcher::{self, LaunchOutcome};
use crate::metadata;
use crate::model::{FfmpegStatus, Library, MediaFile, OpenResult, Rule, Settings};
use crate::state::AppState;

/// 保存类操作的统一返回（对齐旧版 `{ ok, error?, ... }`）
#[derive(Debug, Serialize)]
pub struct SaveResult<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules: Option<T>,
}

#[derive(Debug, Deserialize)]
pub struct MetaQuery {
    pub path: String,
    pub ext: String,
}

// ---------------------------------------------------------------------------
// 媒体库
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn library_get(state: State<'_, AppState>) -> AppResult<Library> {
    Ok(state.library().await)
}

#[tauri::command]
pub async fn library_rescan(state: State<'_, AppState>) -> AppResult<Library> {
    Ok(state.rescan().await)
}

// ---------------------------------------------------------------------------
// 设置与规则
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Settings {
    state.settings_snapshot()
}

#[tauri::command]
pub fn settings_save(
    state: State<'_, AppState>,
    settings: Settings,
) -> SaveResult<Settings> {
    match crate::store::save_settings(&state.paths, &settings) {
        Ok(saved) => {
            *state.settings.lock().unwrap() = saved.clone();
            // 根目录可能变了，缓存作废
            state.invalidate_library();
            SaveResult {
                ok: true,
                error: None,
                settings: Some(saved),
                rules: None,
            }
        }
        Err(e) => SaveResult {
            ok: false,
            error: Some(e.to_string()),
            settings: None,
            rules: None,
        },
    }
}

#[tauri::command]
pub fn rules_get(state: State<'_, AppState>) -> Vec<Rule> {
    state.rules_snapshot()
}

// ---------------------------------------------------------------------------
// 已读标记
// ---------------------------------------------------------------------------

/// 前端启动时拉一次全量已读集合。
///
/// 返回 `Vec<String>` 而不是集合 —— 前端直接 `new Set(...)`，
/// 而 JSON 里数组的顺序对前端无意义，用 `BTreeSet` 只是为了让
/// `read.json` 本身可读。这里保持"集合语义"在类型上最直白。
#[tauri::command]
pub fn read_get(state: State<'_, AppState>) -> Vec<String> {
    state.read_snapshot().into_iter().collect()
}

/// 切换单个文件的已读状态，返回切换后的新状态（true = 已读）
#[tauri::command]
pub fn read_toggle(state: State<'_, AppState>, path: String) -> AppResult<bool> {
    state.toggle_read(&path)
}

/// 批量设置已读状态（右键菜单对当前文件/整组使用）
#[tauri::command]
pub fn read_set(state: State<'_, AppState>, paths: Vec<String>, read: bool) -> AppResult<usize> {
    state.set_read(&paths, read)?;
    Ok(state.read_snapshot().len())
}

/// 清空全部已读标记，返回清掉的条数
#[tauri::command]
pub fn read_clear(state: State<'_, AppState>) -> AppResult<usize> {
    state.clear_read()
}

#[tauri::command]
pub fn rules_save(state: State<'_, AppState>, rules: Vec<Rule>) -> SaveResult<Vec<Rule>> {
    match crate::store::save_rules(&state.paths, &rules) {
        Ok(saved) => {
            *state.rules.lock().unwrap() = saved.clone();
            SaveResult {
                ok: true,
                error: None,
                settings: None,
                rules: Some(saved),
            }
        }
        Err(e) => SaveResult {
            ok: false,
            error: Some(e.to_string()),
            settings: None,
            rules: None,
        },
    }
}

// ---------------------------------------------------------------------------
// 文件操作
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn file_open(
    app: AppHandle,
    state: State<'_, AppState>,
    file: MediaFile,
) -> AppResult<OpenResult> {
    if file.path.is_empty() {
        return Ok(OpenResult {
            ok: false,
            fallback: None,
            reason: None,
            exe: None,
            error: Some("无效的文件参数".to_string()),
        });
    }

    let rules = state.rules_snapshot();
    let matched = launcher::match_rule(&file.ext, file.category.as_deref(), &rules);

    let Some(rule) = matched else {
        // 没命中规则 → 系统默认程序
        return Ok(open_with_default_impl(&app, &file.path, Some("未匹配规则")));
    };

    match launcher::launch(&rule, &file.path) {
        LaunchOutcome::Launched { exe } => Ok(OpenResult {
            ok: true,
            fallback: None,
            reason: None,
            exe: Some(exe),
            error: None,
        }),
        LaunchOutcome::Fallback { reason } => {
            Ok(open_with_default_impl(&app, &file.path, Some(&reason)))
        }
    }
}

#[tauri::command]
pub async fn file_open_default(app: AppHandle, file: MediaFile) -> AppResult<OpenResult> {
    if file.path.is_empty() {
        return Ok(OpenResult {
            ok: false,
            fallback: None,
            reason: None,
            exe: None,
            error: Some("无效的文件参数".to_string()),
        });
    }
    Ok(open_with_default_impl(&app, &file.path, None))
}

/// 用系统默认程序打开，并把 openPath 的错误原样带回前端
fn open_with_default_impl(app: &AppHandle, path: &str, reason: Option<&str>) -> OpenResult {
    match app.opener().open_path(path, None::<&str>) {
        Ok(()) => OpenResult {
            ok: true,
            fallback: Some(true),
            reason: reason.map(|s| s.to_string()),
            exe: None,
            error: None,
        },
        Err(e) => OpenResult {
            ok: false,
            fallback: Some(true),
            reason: reason.map(|s| s.to_string()),
            exe: None,
            error: Some(e.to_string()),
        },
    }
}

#[tauri::command]
pub fn file_reveal(app: AppHandle, path: String) -> AppResult<bool> {
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| AppError::msg(e.to_string()))?;
    Ok(true)
}

#[tauri::command]
pub fn file_copy(app: AppHandle, text: String) -> AppResult<bool> {
    app.clipboard()
        .write_text(text)
        .map_err(|e| AppError::msg(e.to_string()))?;
    Ok(true)
}

#[tauri::command]
pub async fn exe_validate(path: String) -> bool {
    !path.is_empty() && Path::new(&path).is_file()
}

#[tauri::command]
pub async fn dialog_pick_directory(app: AppHandle) -> Option<String> {
    let picked = app.dialog().file().blocking_pick_folder()?;
    picked.into_path().ok().map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn dialog_pick_exe(app: AppHandle) -> Option<String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("可执行程序", &["exe"])
        .blocking_pick_file()?;
    picked.into_path().ok().map(|p| p.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// 视频元数据
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn file_meta(
    state: State<'_, AppState>,
    file: MetaQuery,
) -> AppResult<serde_json::Value> {
    let meta = state.meta.clone();
    let settings = state.settings_snapshot();
    let data_dir = state.data_dir();
    let path = file.path;
    let ext = file.ext;

    // ffprobe 是阻塞调用（最长 12 秒）。必须挪到阻塞线程池：
    // 直接在 async 命令里跑会占住异步工作线程，几十个视频同时请求时
    // 会把 file_thumb 等其它 IPC 一起卡住。
    let duration = tauri::async_runtime::spawn_blocking(move || {
        let mut cache = meta.lock().unwrap();
        metadata::video_duration(&mut cache, &path, &ext, &settings, &data_dir)
    })
    .await
    .unwrap_or(None);

    Ok(serde_json::json!({ "duration": duration }))
}

/// 时长缓存条目数（设置页展示用）
#[tauri::command]
pub fn meta_cache_count(state: State<'_, AppState>) -> usize {
    state.meta.lock().map(|c| c.len()).unwrap_or(0)
}

#[tauri::command]
pub async fn ffmpeg_status(state: State<'_, AppState>) -> AppResult<FfmpegStatus> {
    let settings = state.settings_snapshot();
    let data_dir = state.data_dir();
    let mut cache = state.meta.lock().unwrap();
    Ok(metadata::status(&mut cache, &settings, &data_dir))
}

// ---------------------------------------------------------------------------
// 缩略图
// ---------------------------------------------------------------------------

/// 取缩略图。
///
/// 返回**缩略图文件的绝对路径**（不是文件名）—— 前端交给 `convertFileSrc`
/// 转成 `thumb://` 协议 URL 即可，平台差异由 Tauri 的官方助手函数处理。
///
/// 返回 `None` 的含义：生成不出来（pdf 一定走这里），
/// 渲染层应当后续用 pdf.js 自己渲染并回调 `file_save_thumb`。
#[tauri::command]
pub async fn file_thumb(state: State<'_, AppState>, path: String) -> AppResult<Option<String>> {
    let ctx = state.thumb_ctx();
    match state.thumbs.get_or_create(&path, &ctx).await {
        Some(hash) => Ok(Some(
            state.thumbs.dir().join(hash).to_string_lossy().into_owned(),
        )),
        None => Ok(None),
    }
}

/// 渲染层渲染好缩略图后回传（目前只有 pdf 走这条路）
#[tauri::command]
pub fn file_save_thumb(
    state: State<'_, AppState>,
    path: String,
    data_url: String,
) -> SaveResult<()> {
    let fail = |msg: String| SaveResult::<()> {
        ok: false,
        error: Some(msg),
        settings: None,
        rules: None,
    };

    let Some((_, payload)) = data_url.split_once(";base64,") else {
        return fail("无效的图片数据".to_string());
    };
    if !data_url.starts_with("data:image/") {
        return fail("无效的图片数据".to_string());
    }

    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(payload) else {
        return fail("无效的图片数据".to_string());
    };

    let out = state.thumbs.dir().join(crate::thumbs::ThumbEngine::hash_of(&path));
    if let Err(e) = std::fs::write(&out, bytes) {
        return fail(e.to_string());
    }

    // 清掉失败标记，后续请求就能命中缓存了
    state.thumbs.unfail(&path);

    SaveResult {
        ok: true,
        error: None,
        settings: None,
        rules: None,
    }
}

#[tauri::command]
pub fn thumbs_clear(state: State<'_, AppState>) -> SaveResult<()> {
    state.thumbs.clear();
    SaveResult {
        ok: true,
        error: None,
        settings: None,
        rules: None,
    }
}

#[tauri::command]
pub fn thumbs_count(state: State<'_, AppState>) -> usize {
    state.thumbs.count()
}

// ---------------------------------------------------------------------------
// 原始文件读取（pdf.js 用）
// ---------------------------------------------------------------------------

/// 整份读取的上限，与旧版一致
const MAX_READ_BYTES: u64 = 512 * 1024 * 1024;

/// 单次区间读取的上限。
///
/// pdf.js 会把相邻的缺失 chunk 合并成一次请求，正常路径下单次只有几百 KB；
/// 这个上限只是守卫，超过就 fail fast。
///
/// ⚠️ **绝不能截断返回短包**：worker 侧用 `end = begin + chunk.byteLength` 记账，
/// 短包会让那个 chunk 永久缺失且不重发（`pdf.worker.mjs` 的 `_requestsByChunk`），
/// 该次加载会永久挂起。
const MAX_RANGE_BYTES: u64 = 64 * 1024 * 1024;

/// 收集允许访问的根目录：所有分类根 + 所有手工挂载的作品集。
///
/// 渲染层只应请求媒体库里的文件，所以「落在某个根目录内」是一条正确且足够紧的约束。
/// 这同时补上了一个安全缺口 —— 此前 `read_file_data` 只校验 `is_file()`，
/// 等于给渲染层开了一个任意文件读取的口子。
fn allowed_roots(settings: &Settings) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for dir in settings.categories.values().flatten() {
        if !dir.trim().is_empty() {
            roots.push(PathBuf::from(dir));
        }
    }
    for extra in settings.extra_collections.values().flatten() {
        if !extra.path.trim().is_empty() {
            roots.push(PathBuf::from(&extra.path));
        }
    }
    roots
}

/// 路径校验：`canonicalize`（一并解决 `..`、相对路径与符号链接逃逸）
/// → 必须落在某个根目录内 → 必须是常规文件。
///
/// ⚠️ 这里**刻意不做 `path.contains("..")` 的子串检查** ——
/// 媒体库里 `第01话..修正版.mp4` 这类文件名很常见，子串检查会误杀合法文件。
/// `canonicalize` 已经把 `..` 完全解开了，下面基于「解开的绝对路径是否落在
/// 某个根目录内」的判断才是真正严密的防线，子串检查纯属有害冗余。
fn resolve_allowed_file(path: &str, roots: &[PathBuf]) -> AppResult<PathBuf> {
    if path.trim().is_empty() {
        return Err(AppError::msg("无效的文件路径"));
    }

    // 文件不存在时 canonicalize 直接失败 → 一并把「不可读」挡掉
    let real = Path::new(path)
        .canonicalize()
        .map_err(|_| AppError::msg("文件不可读"))?;

    let allowed = roots.iter().any(|root| {
        root.canonicalize()
            .map(|r| real.starts_with(&r))
            .unwrap_or(false)
    });
    if !allowed {
        return Err(AppError::msg("路径不在媒体库根目录内"));
    }
    if !real.is_file() {
        return Err(AppError::msg("不是文件"));
    }
    Ok(real)
}

/// 读取 `[begin, end)` 的原始字节。
///
/// `end` 是**排他上界**，与 pdf.js `ChunkedStreamManager` 的记账方式严格对应。
/// 用 `read_exact` 而不是 `read`：短读必须报错，不能返回短包。
///
/// 抽成纯函数是为了能脱离 Tauri 用 `cargo test` 验证字节语义。
fn read_range_bytes(path: &Path, begin: u64, end: u64) -> AppResult<Vec<u8>> {
    if begin >= end {
        return Err(AppError::msg("无效的读取区间"));
    }
    if end - begin > MAX_RANGE_BYTES {
        return Err(AppError::msg("读取区间过大"));
    }

    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    if end > len {
        return Err(AppError::msg("读取区间超出文件长度"));
    }

    file.seek(SeekFrom::Start(begin))?;
    let mut buf = vec![0u8; (end - begin) as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// 整份读取文件内容。
///
/// 用 `ipc::Response` 而不是 `Vec<u8>`：后者会被序列化成 JSON 数组
/// （一个字节一个数字），几百 MB 的 PDF 会把内存和 CPU 都吃干净。
///
/// 只服务「小文件」分支（阈值见前端 `src/utils/pdfRange.ts`）；
/// 大文件走 `read_file_range`，避免把整份内容搬进内存再跨 IPC 复制。
#[tauri::command]
pub async fn read_file_data(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<tauri::ipc::Response> {
    let roots = allowed_roots(&state.settings_snapshot());

    // 阻塞 IO 必须挪到阻塞线程池：直接写在 async 命令里会占住 tokio 工作线程
    // （worker 数 = CPU 核数），把其它 IPC 一起拖住。
    let bytes = tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<u8>> {
        let real = resolve_allowed_file(&path, &roots)?;
        let md = std::fs::metadata(&real)?;
        if md.len() > MAX_READ_BYTES {
            return Err(AppError::msg("文件过大，跳过"));
        }
        Ok(std::fs::read(&real)?)
    })
    .await
    .map_err(|e| AppError::msg(format!("读取任务失败: {e}")))??;

    Ok(tauri::ipc::Response::new(bytes))
}

/// 按区间读取文件，供 pdf.js 的 `PDFDataRangeTransport` 使用。
///
/// 走这条路 pdf.js **不会发全量 GET**（`getFullReader()` 只消费我们给的
/// `initialData`），只按 chunk 粒度要数据 —— 渲染一个 90MB PDF 的第一页
/// 通常总共只需几 MB，而不是 90MB。
#[tauri::command]
pub async fn read_file_range(
    state: State<'_, AppState>,
    path: String,
    begin: u64,
    end: u64,
) -> AppResult<tauri::ipc::Response> {
    let roots = allowed_roots(&state.settings_snapshot());

    let bytes = tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<u8>> {
        let real = resolve_allowed_file(&path, &roots)?;
        read_range_bytes(&real, begin, end)
    })
    .await
    .map_err(|e| AppError::msg(format!("读取任务失败: {e}")))??;

    Ok(tauri::ipc::Response::new(bytes))
}

// ---------------------------------------------------------------------------
// 启动握手
// ---------------------------------------------------------------------------

/// 前端挂载完成后调用：显示窗口。
///
/// 窗口在 `tauri.conf.json` 里配的是 `visible: false`，等这里收到信号再显示，
/// 避免白屏闪一下（旧版用的是 Electron 的 `ready-to-show`）。
#[tauri::command]
pub fn ui_ready(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// 数据目录（设置页展示用）
#[tauri::command]
pub fn app_data_dir(state: State<'_, AppState>) -> String {
    state.paths.display()
}

// ---------------------------------------------------------------------------
// 区间读取的字节语义与路径校验（不需要 GUI，cargo test 即可跑）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{read_range_bytes, resolve_allowed_file, MAX_RANGE_BYTES};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    /// 造一个内容可预测的文件：第 i 字节 = i % 251
    fn make_file(dir: &Path, name: &str, len: usize) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        f.write_all(&data).unwrap();
        p
    }

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mediarouter-test-{tag}-{}", std::process::id()))
    }

    #[test]
    fn range_is_half_open_and_byte_exact() {
        let dir = temp_dir("range");
        let file = make_file(&dir, "a.bin", 4096);

        // 全量
        let all = read_range_bytes(&file, 0, 4096).unwrap();
        assert_eq!(all.len(), 4096);
        assert_eq!(all[0], 0);
        assert_eq!(all[4095], (4095 % 251) as u8);

        // [begin, end) —— 不含 end 那一字节，这是与 pdf.js 记账对齐的关键
        let mid = read_range_bytes(&file, 10, 20).unwrap();
        assert_eq!(mid.len(), 10, "长度必须是 end - begin");
        assert_eq!(mid[0], (10 % 251) as u8);
        assert_eq!(mid[9], (19 % 251) as u8, "最后一字节来自 end - 1");

        // 贴尾
        let tail = read_range_bytes(&file, 4090, 4096).unwrap();
        assert_eq!(tail.len(), 6);
        assert_eq!(*tail.last().unwrap(), (4095 % 251) as u8);
    }

    #[test]
    fn range_rejects_bad_spans() {
        let dir = temp_dir("range-bad");
        let file = make_file(&dir, "a.bin", 1024);

        assert!(read_range_bytes(&file, 10, 10).is_err(), "空区间必须报错");
        assert!(read_range_bytes(&file, 20, 10).is_err(), "倒序必须报错");
        assert!(
            read_range_bytes(&file, 0, 1025).is_err(),
            "越界必须报错（短读绝不允许）"
        );
        assert!(
            read_range_bytes(&file, 0, MAX_RANGE_BYTES + 1).is_err(),
            "超单次上限必须报错"
        );
    }

    #[test]
    fn resolve_rejects_outside_and_missing() {
        let root = temp_dir("allow-root");
        let inside = make_file(&root, "ok.bin", 16);
        // 造在 root 的父目录，确保它「不在根内」
        let outside = make_file(&std::env::temp_dir(), "outside-allow.bin", 16);
        let roots = vec![root.clone()];

        assert!(
            resolve_allowed_file(&inside.to_string_lossy(), &roots).is_ok(),
            "根目录内的文件应通过"
        );
        assert!(
            resolve_allowed_file(&outside.to_string_lossy(), &roots).is_err(),
            "根目录外的文件应被拒"
        );
        assert!(
            resolve_allowed_file(&root.join("nope.bin").to_string_lossy(), &roots).is_err(),
            "不存在的文件应被拒"
        );
        assert!(
            resolve_allowed_file("", &roots).is_err(),
            "空路径应被拒"
        );
    }

    #[test]
    fn resolve_defeats_traversal_and_keeps_dotdot_filenames() {
        let root = temp_dir("allow-traverse");
        let roots = vec![root.clone()];

        // 用 `..` 跳出根目录 → canonicalize 之后落在根外 → 拒绝
        let escape = root.join("..").join("outside-allow.bin");
        assert!(
            resolve_allowed_file(&escape.to_string_lossy(), &roots).is_err(),
            "基于 .. 的穿越必须被拒"
        );

        // 反过来：文件名里带 `..` 是合法的，不能被误杀
        let weird = make_file(&root, "第01话..修正版.bin", 8);
        assert!(
            resolve_allowed_file(&weird.to_string_lossy(), &roots).is_ok(),
            "文件名含 .. 不应被误拒"
        );
    }
}
