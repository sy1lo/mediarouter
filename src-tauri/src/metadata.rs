//! 视频元数据（ffmpeg / ffprobe 探测与时长查询）。
//!
//! 与旧版 `electron/metadata.js` 的探测顺序一致，但有两处实质改动：
//!
//! 1. 去掉了一个已失效的分支：旧版第三个候选是 `PORTABLE_EXECUTABLE_DIR`
//!    环境变量——它由 NSIS 便携壳注入，换成 zip / 安装包分发后永远为空，
//!    而且「exe 所在目录」本来就已经覆盖了它的语义。
//! 2. **时长缓存落盘**。旧版（以及本版最初的实现）都只存在内存里，
//!    于是每次重启应用都要对每个视频重跑一遍 ffprobe——一个 ffprobe 进程
//!    的启动加解析在几百毫秒量级，几十个视频同时来会把异步运行时的工作线程
//!    占满，连带把缩略图请求一起拖住。现在缓存在 `<数据目录>/meta.json`。
//!
//! 另外把旧版的 `spawnSync('where', ['ffmpeg'])` 换成直接扫 PATH——
//! 少起一个进程，效果相同。

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::model::{FfmpegStatus, Settings};
use crate::paths;
use crate::platform::hide_console;

/// 需要走 ffprobe 的扩展名
pub const VIDEO_EXTS: [&str; 10] = [
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "ts", "rmvb",
];

/// ffprobe 超时（旧版是 12s）
const PROBE_TIMEOUT: Duration = Duration::from_secs(12);

/// 时长缓存落盘文件名
const META_FILE: &str = "meta.json";

/// 落盘节流：最多这么久写一次，避免每个文件都触发一次磁盘写
const FLUSH_INTERVAL: Duration = Duration::from_secs(2);

/// 磁盘格式版本。改了 `DurationEntry` 的结构就要递增，否则旧文件会被丢弃。
const META_VERSION: u32 = 1;

/// 磁盘上的时长缓存
#[derive(Debug, Default, Serialize, Deserialize)]
struct MetaFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    durations: HashMap<String, DurationEntry>,
}

/// 单个文件的时长记录。
///
/// 用 **mtime + size** 而不是只用路径做失效判定：视频被重新压制后时长会变，
/// 只看路径会一直返回过期值。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DurationEntry {
    mtime: f64,
    size: u64,
    /// `None` 表示探过了但拿不到（避免每次都重试）
    duration: Option<f64>,
}

pub struct MetaCache {
    durations: HashMap<String, DurationEntry>,
    ffmpeg_dir: Option<PathBuf>,
    ffmpeg_source: Option<String>,
    ffmpeg_checked: bool,
    dirty: bool,
    last_flush: Option<Instant>,
}

impl Default for MetaCache {
    fn default() -> Self {
        Self {
            durations: HashMap::new(),
            ffmpeg_dir: None,
            ffmpeg_source: None,
            ffmpeg_checked: false,
            dirty: false,
            last_flush: None,
        }
    }
}

impl MetaCache {
    /// 从数据目录载入时长缓存
    pub fn load(data_dir: &Path) -> Self {
        let mut cache = Self::default();
        if let Some(payload) = crate::store::load_json::<MetaFile>(&data_dir.join(META_FILE)) {
            if payload.version == META_VERSION {
                cache.durations = payload.durations;
            }
        }
        cache
    }

    /// 立即落盘（应用退出时也调一次，避免节流窗口内丢数据）
    pub fn flush(&mut self, data_dir: &Path) {
        if !self.dirty {
            return;
        }
        let payload = MetaFile {
            version: META_VERSION,
            durations: std::mem::take(&mut self.durations),
        };
        match crate::store::write_json_atomic(&data_dir.join(META_FILE), &payload) {
            Ok(()) => {
                self.durations = payload.durations;
                self.dirty = false;
                self.last_flush = Some(Instant::now());
            }
            Err(_) => {
                // 写失败就把数据还回去，下次再试
                self.durations = payload.durations;
            }
        }
    }

    /// 节流落盘
    fn maybe_flush(&mut self, data_dir: &Path) {
        if !self.dirty {
            return;
        }
        let due = self
            .last_flush
            .map(|t| t.elapsed() >= FLUSH_INTERVAL)
            .unwrap_or(true);
        if due {
            self.flush(data_dir);
        }
    }

    /// 已缓存的条目数（供设置页展示）
    pub fn len(&self) -> usize {
        self.durations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.durations.is_empty()
    }
}

fn mtime_ms(md: &std::fs::Metadata) -> f64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// 在 PATH 中查找可执行文件（替代 `where.exe`）
fn find_in_path(exe: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(exe))
        .find(|cand| cand.is_file())
}

/// 在 base 目录下找形如 `ffmpeg*` 的文件夹，返回其中含 ffmpeg/ffprobe 的那个。
///
/// 两种常见布局都认：扁平（`<dir>/ffmpeg.exe`）和官方发行包
/// （`<dir>/bin/ffmpeg.exe`）。
fn probe_layout_dir(base: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(base).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !name.starts_with("ffmpeg") {
            continue;
        }
        let dir = entry.path();
        for cand in [dir.clone(), dir.join("bin")] {
            if cand.join("ffmpeg.exe").is_file() || cand.join("ffprobe.exe").is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// 探测 ffmpeg 所在目录。返回值同时会写回 cache。
pub fn detect_ffmpeg_dir(
    cache: &mut MetaCache,
    settings: &Settings,
    data_dir: &Path,
) -> Option<PathBuf> {
    // 1. 用户在设置里显式指定
    if !settings.ffmpeg_path.is_empty() {
        let p = PathBuf::from(&settings.ffmpeg_path);
        // 用户可能填到发行包根目录（ffmpeg.exe 在 bin/ 下），两种都认
        for cand in [p.clone(), p.join("bin")] {
            if cand.join("ffmpeg.exe").is_file() || cand.join("ffprobe.exe").is_file() {
                cache.ffmpeg_dir = Some(cand.clone());
                cache.ffmpeg_source = Some("settings".to_string());
                return Some(cand);
            }
        }
    }

    // 2/3/4 只做一次
    if cache.ffmpeg_checked {
        return cache.ffmpeg_dir.clone();
    }
    cache.ffmpeg_checked = true;

    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(d) = paths::exe_dir() {
        bases.push(d);
    }
    bases.push(data_dir.to_path_buf());

    for base in bases {
        if let Some(dir) = probe_layout_dir(&base) {
            cache.ffmpeg_dir = Some(dir.clone());
            cache.ffmpeg_source = Some("auto".to_string());
            return Some(dir);
        }
    }

    if let Some(exe) = find_in_path("ffmpeg.exe") {
        if let Some(dir) = exe.parent() {
            let dir = dir.to_path_buf();
            cache.ffmpeg_dir = Some(dir.clone());
            cache.ffmpeg_source = Some("path".to_string());
            return Some(dir);
        }
    }

    None
}

pub fn ffmpeg_exe(cache: &mut MetaCache, settings: &Settings, data_dir: &Path) -> Option<PathBuf> {
    let dir = detect_ffmpeg_dir(cache, settings, data_dir)?;
    // 同样兼容扁平与 bin/ 两种布局
    for p in [dir.join("ffmpeg.exe"), dir.join("bin").join("ffmpeg.exe")] {
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub fn ffprobe_exe(cache: &mut MetaCache, settings: &Settings, data_dir: &Path) -> Option<PathBuf> {
    let dir = detect_ffmpeg_dir(cache, settings, data_dir)?;
    for p in [dir.join("ffprobe.exe"), dir.join("bin").join("ffprobe.exe")] {
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// 查询视频时长（秒）。
///
/// 非视频扩展名、文件不存在、或找不到 ffprobe 时返回 `None`。
/// 命中条件为 **mtime 与 size 都没变**；未命中才起 ffprobe，并写回缓存。
pub fn video_duration(
    cache: &mut MetaCache,
    file_path: &str,
    ext: &str,
    settings: &Settings,
    data_dir: &Path,
) -> Option<f64> {
    if !VIDEO_EXTS.contains(&ext.to_lowercase().as_str()) {
        return None;
    }

    let md = std::fs::metadata(file_path).ok()?;
    let (mtime, size) = (mtime_ms(&md), md.len());

    // 先查缓存（这才是「重启不用重新检测」的关键）
    if let Some(entry) = cache.durations.get(file_path) {
        if entry.mtime == mtime && entry.size == size {
            return entry.duration;
        }
    }

    let probe = ffprobe_exe(cache, settings, data_dir)?;
    let value = probe_duration(&probe, file_path);

    cache.durations.insert(
        file_path.to_string(),
        DurationEntry {
            mtime,
            size,
            duration: value,
        },
    );
    cache.dirty = true;
    cache.maybe_flush(data_dir);

    value
}

/// 直接探测时长（不走缓存）。缩略图抽帧定位也复用它。
pub fn probe_duration(probe: &Path, file_path: &str) -> Option<f64> {
    let mut cmd = Command::new(probe);
    cmd.args([
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "csv=p=0",
        file_path,
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .stdin(Stdio::null());
    hide_console(&mut cmd);

    run_capturing_stdout(cmd, PROBE_TIMEOUT)
        .and_then(|out| out.trim().parse::<f64>().ok().filter(|v| v.is_finite()))
}

/// 启动子进程并等待结束，超时则杀掉。
///
/// ⚠️ 这里刻意用轮询 `try_wait`，**不要**改成 `child.stdout.take()`：
/// 后者在调用方把 stdout 设成 `Stdio::null()` 时拿到的是 `None`，
/// 会让整个函数在起步处就短路返回 `None`。
/// 曾经因为这一点，ffmpeg 抽帧永远「失败」，所有视频缩略图都退化成
/// 系统缩略图（无损 PNG，实测体积差 51 倍）。
pub fn run_to_completion(mut cmd: Command, timeout: Duration) -> Option<ExitStatus> {
    let mut child = cmd.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// 启动子进程、捕获 stdout 并在超时后杀掉。
///
/// **要求调用方把 stdout 设为 `Stdio::piped()`**，否则 `child.stdout` 为 `None`，
/// 函数会直接返回 `None`。
pub fn run_capturing_stdout(mut cmd: Command, timeout: Duration) -> Option<String> {
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        let _ = tx.send(buf);
    });

    match rx.recv_timeout(timeout) {
        Ok(text) => {
            let _ = child.wait();
            Some(text)
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

pub fn status(cache: &mut MetaCache, settings: &Settings, data_dir: &Path) -> FfmpegStatus {
    let dir = detect_ffmpeg_dir(cache, settings, data_dir);
    let ffmpeg = ffmpeg_exe(cache, settings, data_dir);
    let probe = ffprobe_exe(cache, settings, data_dir);
    FfmpegStatus {
        ready: probe.is_some(),
        dir: dir.map(|d| d.to_string_lossy().into_owned()),
        source: cache.ffmpeg_source.clone(),
        ffmpeg: ffmpeg.map(|p| p.to_string_lossy().into_owned()),
        ffprobe: probe.map(|p| p.to_string_lossy().into_owned()),
    }
}
