//! 少量与平台强相关的工具。

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Unix 毫秒（对齐 JS 的 `Date.now()` / `stats.mtimeMs`）
pub fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 让子进程不弹控制台窗口（等价 Node 的 `windowsHide: true`）
pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}
