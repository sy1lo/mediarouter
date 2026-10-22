//! 与前端共享的数据模型。
//!
//! 字段名刻意与旧版 Electron 返回的 JSON 完全一致（含 camelCase 重命名），
//! 这样前端的 TS 接口可以原样保留，不需要做字段映射。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 媒体库分类（顺序即侧边栏展示顺序，前端使用同名常量）
pub const CATEGORIES: [&str; 4] = ["动漫", "漫画", "小说", "图片"];

/// 单个媒体文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFile {
    pub name: String,
    /// 绝对路径
    pub path: String,
    /// 小写扩展名，不含前导点
    pub ext: String,
    pub size: u64,
    /// 修改时间，Unix 毫秒（对齐 JS 的 `stats.mtimeMs`）
    pub mtime: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// 图片的原始宽度。
    ///
    /// 只有「图片」分类会带这两个字段（扫描时读文件头得到的，见 `imagedims`）。
    /// 用途：三级菜单的平铺布局**必须在渲染前就知道宽高比**才能一次算准，
    /// 否则只能先按占位比例排一遍、等图片加载完再重排 —— 表现就是
    /// 「打开文件夹后版面整体跳一下」。其他分类用不到，因此不填、也不出现在 JSON 里。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// 图片的原始高度，与 `width` 同时出现
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

impl MediaFile {
    pub fn from_path(path: std::path::PathBuf, category: &str, size: u64, mtime: f64) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ext = path
            .extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        Self {
            name,
            path: path.to_string_lossy().into_owned(),
            ext,
            size,
            mtime,
            category: Some(category.to_string()),
            width: None,
            height: None,
        }
    }
}

/// 作品集（一个目录，或根目录下的散落文件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    /// `dir:<绝对路径>` / `root:<根目录>` / `extra:<绝对路径>`
    pub id: String,
    pub name: String,
    pub path: String,
    pub count: usize,
    #[serde(rename = "totalSize")]
    pub total_size: u64,
    pub files: Vec<MediaFile>,
    /// 根目录下的散落文件集合
    #[serde(default)]
    pub loose: bool,
    /// 用户手工挂载的额外作品集
    #[serde(default)]
    pub extra: bool,
}

/// 单个分类的扫描结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryData {
    pub roots: Vec<String>,
    pub collections: Vec<Collection>,
}

/// 整库扫描结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub categories: BTreeMap<String, CategoryData>,
    /// Unix 毫秒
    #[serde(rename = "scannedAt")]
    pub scanned_at: f64,
}

/// 用户配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub categories: BTreeMap<String, Vec<String>>,
    #[serde(rename = "extraCollections")]
    pub extra_collections: BTreeMap<String, Vec<ExtraCollection>>,
    #[serde(rename = "ffmpegPath", default)]
    pub ffmpeg_path: String,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    "dark".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        let mut categories = BTreeMap::new();
        let mut extra = BTreeMap::new();
        for c in CATEGORIES {
            categories.insert(c.to_string(), Vec::new());
            extra.insert(c.to_string(), Vec::new());
        }
        Self {
            categories,
            extra_collections: extra,
            ffmpeg_path: String::new(),
            theme: default_theme(),
        }
    }
}

impl Settings {
    /// 用默认值补齐缺失字段（对齐旧版 `normalize()`）
    pub fn normalize(input: Option<Settings>) -> Self {
        let mut merged = input.unwrap_or_default();
        for c in CATEGORIES {
            merged.categories.entry(c.to_string()).or_default();
            merged.extra_collections.entry(c.to_string()).or_default();
        }
        if merged.theme.is_empty() {
            merged.theme = default_theme();
        }
        merged
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraCollection {
    pub name: String,
    pub path: String,
}

/// 一条路由规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub exe: String,
    #[serde(default)]
    pub args: String,
    /// 旧版判定是 `r.enabled === false`，即缺省视为启用
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// ffmpeg 探测结果
#[derive(Debug, Clone, Serialize)]
pub struct FfmpegStatus {
    pub ready: bool,
    pub dir: Option<String>,
    pub source: Option<String>,
    pub ffmpeg: Option<String>,
    pub ffprobe: Option<String>,
}

/// 打开文件的返回结构
#[derive(Debug, Clone, Serialize)]
pub struct OpenResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
