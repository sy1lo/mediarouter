//! 缩略图引擎。
//!
//! 与旧版 `electron/thumbs.js` 对齐的部分：
//! - 缓存文件名 = `md5(绝对路径 + "\0" + THUMB_VERSION)`，
//!   **算法与 THUMB_VERSION 都没动**，所以已有的几百张缩略图可以直接继承
//! - `.version` 不匹配则整目录作废重建
//! - `failed` 集合：生成不出来的文件只试一次，避免反复重试
//! - pdf 后端不生成，返回 `None` 交给渲染层的 pdf.js（渲染完再回调 `save_thumb`）
//!
//! 相比旧版的三处改进：
//! 1. 并发上限从 2 提到 4 —— 旧版压到 2 是为了控 Node 侧的内存峰值，
//!    Rust 每张图的驻留内存小一个量级，没必要继续付这个代价
//! 2. 去重方式从「队列 + 单例 promise」换成「拿到信号量后复查磁盘」，
//!    无锁、无状态，并发请求同一文件时后到的那个直接命中文件
//! 3. 系统缩略图不再起 PowerShell 进程（见 `shell.rs`）

pub mod epub;
pub mod mime;
pub mod shell;

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use md5::{Digest, Md5};
use zip::ZipArchive;

use crate::model::Settings;
use crate::platform::hide_console;

/// 缓存版本。改动生成逻辑时必须递增，否则旧图不会被重建。
///
/// v2 → v3：修掉了「ffmpeg 抽帧永远失败」的 bug（`run_with_timeout` 在
/// stdout 为 null 时直接短路返回），此前所有视频缩略图都退化成了系统缩略图，
/// 实测体积相差 50 倍（16.9KB JPEG vs 874KB PNG）。必须作废重建才生效。
pub const THUMB_VERSION: &str = "v3";

/// 系统缩略图请求的边长
pub const THUMB_SIZE: u32 = 512;

/// 超过此大小的压缩包直接放弃（避免把几百 MB 的包读进内存）
pub(crate) const MAX_ZIP_SIZE: u64 = 300 * 1024 * 1024;

/// ffmpeg 抽帧超时（旧版 25s）
const FFMPEG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25);

/// 并发上限
const MAX_CONCURRENCY: usize = 4;

/// 生成一张缩略图所需的全部外部上下文。
///
/// 提前算好、整体移进 `spawn_blocking`，避免在阻塞线程里再去抢锁。
#[derive(Clone)]
pub struct ThumbCtx {
    pub settings: Settings,
    pub data_dir: PathBuf,
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
}

pub struct ThumbEngine {
    dir: PathBuf,
    sem: tokio::sync::Semaphore,
    failed: Mutex<std::collections::HashSet<String>>,
}

impl ThumbEngine {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            sem: tokio::sync::Semaphore::new(MAX_CONCURRENCY),
            failed: Mutex::new(std::collections::HashSet::new()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 缓存文件名。算法必须与旧版一致，否则已有缓存全部失效。
    pub fn hash_of(file_path: &str) -> String {
        let mut hasher = Md5::new();
        hasher.update(file_path.as_bytes());
        hasher.update(b"\x00");
        hasher.update(THUMB_VERSION.as_bytes());
        let digest = hasher.finalize();
        let mut out = String::with_capacity(32);
        for b in digest {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }

    fn version_file(&self) -> PathBuf {
        self.dir.join(".version")
    }

    /// 版本不符则清空缓存目录
    pub fn check_version(&self) {
        let current = fs::read_to_string(self.version_file())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if current != THUMB_VERSION {
            self.clear();
        }
    }

    pub fn clear(&self) {
        self.failed.lock().unwrap().clear();
        let _ = fs::remove_dir_all(&self.dir);
        if fs::create_dir_all(&self.dir).is_ok() {
            let _ = fs::write(self.version_file(), THUMB_VERSION);
        }
    }

    /// 已缓存的缩略图张数（不含 `.version`）
    pub fn count(&self) -> usize {
        match fs::read_dir(&self.dir) {
            Ok(entries) => entries
                .flatten()
                .filter(|e| e.file_name() != ".version")
                .count(),
            Err(_) => 0,
        }
    }

    /// 清掉失败标记（渲染层成功生成 PDF 缩略图后会调）
    pub fn unfail(&self, file_path: &str) {
        self.failed.lock().unwrap().remove(&Self::hash_of(file_path));
    }

    /// 取缩略图：命中缓存直接返回文件名，否则生成。
    ///
    /// 返回 `Some(hash)` 表示缓存目录下已存在该文件；`None` 表示生成不出来
    /// （pdf 会走到这里，交给渲染层）。
    pub async fn get_or_create(&self, file_path: &str, ctx: &ThumbCtx) -> Option<String> {
        let hash = Self::hash_of(file_path);

        if self.failed.lock().unwrap().contains(&hash) {
            return None;
        }
        let out = self.dir.join(&hash);
        if out.is_file() {
            return Some(hash);
        }

        // 并发闸门
        let _permit = self.sem.acquire().await.ok()?;

        // 拿到许可后复查：并发请求同一文件时，先完成的那个已经把图写出来了
        if out.is_file() {
            return Some(hash);
        }

        let file_owned = file_path.to_string();
        let out_owned = out.clone();
        let ctx_owned = ctx.clone();

        let ok = tokio::task::spawn_blocking(move || generate(&file_owned, &out_owned, &ctx_owned))
            .await
            .unwrap_or(false);

        if ok && out.is_file() {
            Some(hash)
        } else {
            self.failed.lock().unwrap().insert(hash);
            None
        }
    }
}

// ---------------------------------------------------------------------------
// 归档辅助
// ---------------------------------------------------------------------------

/// 扩展名是否为图片
pub(crate) fn is_image(name: &str) -> bool {
    let lower = name.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]
        .iter()
        .any(|e| lower.ends_with(e))
}

/// 打开压缩包（带大小闸门）
pub(crate) fn open_archive(zip_path: &str) -> Option<ZipArchive<File>> {
    let file = File::open(zip_path).ok()?;
    let md = file.metadata().ok()?;
    if md.len() > MAX_ZIP_SIZE {
        return None;
    }
    ZipArchive::new(file).ok()
}

/// 第一个可用图片条目的下标（跳过目录与 __MACOSX）
pub(crate) fn first_image_index(zip: &mut ZipArchive<File>) -> Option<usize> {
    for i in 0..zip.len() {
        let Ok(entry) = zip.by_index(i) else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if name.starts_with("__MACOSX") {
            continue;
        }
        if is_image(&name) {
            return Some(i);
        }
    }
    None
}

/// 把指定下标的条目原样写出
pub(crate) fn write_index_to(zip: &mut ZipArchive<File>, index: usize, out: &Path) -> bool {
    let Ok(mut entry) = zip.by_index(index) else {
        return false;
    };
    if entry.is_dir() {
        return false;
    }
    let mut buf = Vec::with_capacity(entry.size() as usize);
    if entry.read_to_end(&mut buf).is_err() {
        return false;
    }
    fs::write(out, &buf).is_ok()
}

/// 按条目名写出（epub 里绝大多数路径是 POSIX 风格，直接按名查）
pub(crate) fn write_entry_to(zip: &mut ZipArchive<File>, entry_name: &str, out: &Path) -> bool {
    let Ok(mut entry) = zip.by_name(entry_name) else {
        return false;
    };
    if entry.is_dir() || !is_image(entry.name()) {
        return false;
    }
    let mut buf = Vec::with_capacity(entry.size() as usize);
    if entry.read_to_end(&mut buf).is_err() {
        return false;
    }
    fs::write(out, &buf).is_ok()
}

/// 读取文本条目（UTF-8 宽松解码，对齐 adm-zip 的 readAsText）
pub(crate) fn read_text(zip: &mut ZipArchive<File>, entry_name: &str) -> Option<String> {
    let mut entry = zip.by_name(entry_name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// 从压缩包提取首图（zip / cbz）
pub(crate) fn extract_first_image(zip_path: &str, out: &Path) -> bool {
    let Some(mut zip) = open_archive(zip_path) else {
        return false;
    };
    let Some(idx) = first_image_index(&mut zip) else {
        return false;
    };
    write_index_to(&mut zip, idx, out)
}

// ---------------------------------------------------------------------------
// 生成
// ---------------------------------------------------------------------------

fn ext_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// 生成一张缩略图。返回 true 表示 `out` 已写出。
fn generate(file_path: &str, out: &Path, ctx: &ThumbCtx) -> bool {
    // 清掉可能存在的半截文件，避免「存在但损坏」被当成命中
    let _ = fs::remove_file(out);

    let ext = ext_of(file_path);

    match ext.as_str() {
        "epub" => {
            if epub::extract_cover(file_path, out) {
                return true;
            }
        }
        "zip" | "cbz" => {
            if extract_first_image(file_path, out) {
                return true;
            }
        }
        // pdf 交给渲染层的 pdf.js —— WebView2 里有 canvas 和 worker，
        // 比在 Rust 侧拖进一个 PDF 渲染器划算得多
        "pdf" => return false,
        _ => {}
    }

    if crate::metadata::VIDEO_EXTS.contains(&ext.as_str()) {
        if let Some(ffmpeg) = &ctx.ffmpeg {
            let seek = compute_seek(file_path, ctx);
            if ffmpeg_thumb(ffmpeg, file_path, out, seek) {
                return true;
            }
        }
    }

    // 最后一档：系统缩略图（字幕、cbr、cbt、mobi、azw3、独立图片…）
    shell::system_thumbnail(Path::new(file_path), out, THUMB_SIZE)
}

/// 抽帧位置：取时长的 10%，夹在 [1, 300] 秒；拿不到时长就用第 3 秒
fn compute_seek(file_path: &str, ctx: &ThumbCtx) -> u32 {
    if let Some(probe) = &ctx.ffprobe {
        if let Some(dur) = crate::metadata::probe_duration(probe, file_path) {
            let s = (dur * 0.1).floor();
            return s.clamp(1.0, 300.0) as u32;
        }
    }
    3
}

fn ffmpeg_thumb(ffmpeg: &Path, file_path: &str, out: &Path, seek: u32) -> bool {
    let mut cmd = std::process::Command::new(ffmpeg);
    cmd.args([
        "-y",
        "-ss",
        &seek.to_string(),
        "-i",
        file_path,
        "-f",
        "image2",
        "-vframes",
        "1",
        "-vf",
        "scale=512:-2",
        "-q:v",
        "4",
        &out.to_string_lossy(),
    ])
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
    hide_console(&mut cmd);

    // 必须用 run_to_completion（轮询 try_wait）而不是捕获 stdout 的版本：
    // 这里 stdout 设成了 null，捕获版会在 `child.stdout.take()` 处拿到 None
    // 直接返回 None，导致抽帧「永远失败」。
    let ok = match crate::metadata::run_to_completion(cmd, FFMPEG_TIMEOUT) {
        Some(status) => status.success(),
        None => false,
    };
    ok && out.metadata().map(|m| m.len() > 0).unwrap_or(false)
}
