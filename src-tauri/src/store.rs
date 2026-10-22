//! 配置持久化：`settings.json` / `rules.json`。
//!
//! 与旧版一致：写入走「临时文件 + 原子 rename」，避免半截文件。
//! 差异：旧版是「读一次驻留内存」，这里同样驻留，但由 `AppState` 统一持有。

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppResult;
use crate::model::{ExtraCollection, Rule, Settings};
use crate::paths::DATA_DIR_NAME;

/// 数据目录下的各子路径
#[derive(Debug, Clone)]
pub struct DataPaths {
    pub root: PathBuf,
}

impl DataPaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn rules_file(&self) -> PathBuf {
        self.root.join("rules.json")
    }

    /// 已读标记单独存一个文件。
    ///
    /// 刻意不并进 `settings.json`：已读状态变动频繁（点一次文件就写一次），
    /// 混进 settings 会让每次标记都重写整份配置，还会把「根目录列表」这类
    /// 低频但关键的数据一起卷进高频写路径。
    pub fn read_file(&self) -> PathBuf {
        self.root.join("read.json")
    }

    pub fn thumbs_dir(&self) -> PathBuf {
        self.root.join("thumbs")
    }

    /// 建目录。返回是否创建成功。
    pub fn ensure_dirs(&self) -> bool {
        fs::create_dir_all(self.thumbs_dir()).is_ok()
    }

    /// 供界面展示用的路径描述
    pub fn display(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }
}

/// 读取 JSON，失败一律回退（与旧版 `loadJson` 的容错语义一致）
pub(crate) fn load_json<T: serde::de::DeserializeOwned>(file: &Path) -> Option<T> {
    let raw = fs::read(file).ok()?;
    // 去掉可能的 UTF-8 BOM
    let raw = if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &raw[3..]
    } else {
        raw.as_slice()
    };
    serde_json::from_slice(raw).ok()
}

/// 原子写入（临时文件 + rename），供 meta.json 等一并复用
pub(crate) fn write_json_atomic<T: serde::Serialize>(file: &Path, data: &T) -> AppResult<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = file.with_extension("json.tmp");
    let text = serde_json::to_vec_pretty(data)?;
    fs::write(&tmp, &text)?;
    fs::rename(&tmp, file)?;
    Ok(())
}

pub fn load_settings(paths: &DataPaths) -> Settings {
    Settings::normalize(load_json::<Settings>(&paths.settings_file()))
}

pub fn save_settings(paths: &DataPaths, settings: &Settings) -> AppResult<Settings> {
    let normalized = Settings::normalize(Some(settings.clone()));
    write_json_atomic(&paths.settings_file(), &normalized)?;
    Ok(normalized)
}

pub fn load_rules(paths: &DataPaths) -> Vec<Rule> {
    load_json::<Vec<Rule>>(&paths.rules_file()).unwrap_or_default()
}

pub fn save_rules(paths: &DataPaths, rules: &[Rule]) -> AppResult<Vec<Rule>> {
    // 注意不能直接写 `write_json_atomic(&file, rules)`：
    // 那样 T 会被推断成不定长的 `[Rule]`（&[Rule] 的自动解引用），触发 E0277。
    let owned = rules.to_vec();
    write_json_atomic(&paths.rules_file(), &owned)?;
    Ok(owned)
}

/// 已读标记：文件绝对路径的集合。
///
/// 用 `BTreeSet` 而不是 `HashSet`：序列化出来按路径有序，`read.json`
/// 直接拿给人看/做 diff 都是可读的，不会每次保存都乱序刷屏。
pub type ReadSet = std::collections::BTreeSet<String>;

pub fn load_read(paths: &DataPaths) -> ReadSet {
    load_json::<ReadSet>(&paths.read_file()).unwrap_or_default()
}

pub fn save_read(paths: &DataPaths, read: &ReadSet) -> AppResult<()> {
    write_json_atomic(&paths.read_file(), read)
}

/// 首次运行时写一份默认配置，方便用户手改
pub fn bootstrap_if_missing(paths: &DataPaths) {
    if !paths.settings_file().exists() {
        let defaults = Settings::default();
        let _ = write_json_atomic(&paths.settings_file(), &defaults);
    }
    if !paths.rules_file().exists() {
        let empty: Vec<Rule> = Vec::new();
        let _ = write_json_atomic(&paths.rules_file(), &empty);
    }
    if !paths.read_file().exists() {
        let empty: ReadSet = ReadSet::new();
        let _ = write_json_atomic(&paths.read_file(), &empty);
    }
}

/// 便于外部构造默认配置（设置页「恢复默认」用）
pub fn default_settings() -> Settings {
    Settings::default()
}

/// 便于外部构造空的额外作品集
pub fn empty_extras() -> Vec<ExtraCollection> {
    Vec::new()
}

/// 供启动日志使用
pub fn describe(root: &Path) -> String {
    format!("{} ({})", root.join(DATA_DIR_NAME).display(), root.display())
}
