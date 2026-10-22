//! 数据目录解析。
//!
//! 策略与旧版 Electron 一致（绿色版语义），但判定依据从
//! `PORTABLE_EXECUTABLE_DIR` 环境变量换成了**可执行文件自身位置** ——
//! 环境变量是 NSIS 便携壳注入的，换成 zip / 安装包分发后不再存在。
//!
//! 优先级：
//!   1. `<exe 所在目录>/MediaRouterData`（可写即可用）
//!   2. Tauri 的 app_data_dir（`%APPDATA%\com.medialib.launcher`）
//!
//! 这样「整个文件夹搬走」的绿色语义得以保留，同时避免把几百张缩略图
//! 塞进用户漫游目录。

use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// 数据目录名
pub const DATA_DIR_NAME: &str = "MediaRouterData";

/// 测试目录是否真的可写（仅判断存在性不够，Program Files 下会踩坑）
fn is_writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-probe");
    match fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 解析出本次运行使用的数据目录
pub fn resolve_data_dir(app: &AppHandle) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join(DATA_DIR_NAME);
            if is_writable(&candidate) {
                return candidate;
            }
        }
    }

    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join(DATA_DIR_NAME))
}

/// 可执行文件所在目录（ffmpeg 自动探测用）
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}
