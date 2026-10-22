use serde::Serialize;

/// 统一错误类型。
///
/// 时间点：所有 `#[tauri::command]` 都返回 `Result<T, AppError>`，
/// 前端拿到的就是一个字符串 message（与旧版 Electron 的
/// `{ ok: false, error: err.message }` 语义对齐）。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("文件操作失败: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// 供业务层直接抛出的可读错误
    #[error("{0}")]
    Message(String),
}

impl AppError {
    pub fn msg(s: impl Into<String>) -> Self {
        AppError::Message(s.into())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// 便利宏：`bail!("...")` 相当于 `return Err(AppError::msg(...))`
#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::AppError::Message(format!($($arg)*)))
    };
}
