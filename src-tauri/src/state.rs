//! 应用级共享状态。
//!
//! 旧版把状态散在 `store.js` 的模块级变量 + `scanner.js` 的 `cache` 里；
//! 这里收拢成一个由 Tauri 托管的 `AppState`，好处是初始化顺序显式、可测。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::error::AppResult;
use crate::metadata::MetaCache;
use crate::model::{Library, Rule, Settings};
use crate::scanner;
use crate::store::{self, DataPaths, ReadSet};
use crate::thumbs::{ThumbCtx, ThumbEngine};

pub struct AppState {
    pub paths: DataPaths,
    pub settings: Mutex<Settings>,
    pub rules: Mutex<Vec<Rule>>,
    /// 已读文件的绝对路径集合。
    ///
    /// 常驻内存 + 变更时落盘：标记已读是高频操作（用户可能连着点一串文件），
    /// 每次读盘会把点击延迟拉高到毫秒级 IO 上。集合本身很小（一个路径
    /// 几十字节，一万个文件也就几百 KB），驻留完全无压力。
    pub read: Mutex<ReadSet>,
    pub library: Mutex<Option<Library>>,
    /// 扫描串行化：主进程后台预扫与渲染层首个 `library:get` 会并发触发，
    /// 用同一把异步锁把第二次挡在外面（对齐旧版 scanner.js 的单例 promise）
    scan_lock: tokio::sync::Mutex<()>,
    /// 用 `Arc` 是为了能整体移进 `spawn_blocking`：
    /// ffprobe 是阻塞调用（最长 12 秒），直接在 async 命令里跑会占着
    /// 异步工作线程，把 `file_thumb` 等请求一起拖住。
    pub meta: Arc<Mutex<MetaCache>>,
    pub thumbs: ThumbEngine,
}

impl AppState {
    /// 建目录 → 补默认配置 → 载入设置与规则 → 初始化缩略图缓存
    pub fn bootstrap(root: PathBuf) -> Self {
        let paths = DataPaths::new(root);
        paths.ensure_dirs();
        store::bootstrap_if_missing(&paths);

        let settings = store::load_settings(&paths);
        let rules = store::load_rules(&paths);
        let read = store::load_read(&paths);

        let thumbs = ThumbEngine::new(paths.thumbs_dir());
        // 缓存版本不符就整目录作废重建
        thumbs.check_version();

        // 时长缓存从磁盘载入 —— 这是「重启后不必重新检测时长」的关键
        let meta = MetaCache::load(&paths.root);
        println!("[mediarouter] 时长缓存已载入 {} 条", meta.len());

        Self {
            paths,
            settings: Mutex::new(settings),
            rules: Mutex::new(rules),
            read: Mutex::new(read),
            library: Mutex::new(None),
            scan_lock: tokio::sync::Mutex::new(()),
            meta: Arc::new(Mutex::new(meta)),
            thumbs,
        }
    }

    /// 把时长缓存落盘（应用退出时调用，避免落盘节流窗口内的改动丢失）
    pub fn flush_meta(&self) {
        if let Ok(mut cache) = self.meta.lock() {
            cache.flush(&self.paths.root);
        }
    }

    pub fn settings_snapshot(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn rules_snapshot(&self) -> Vec<Rule> {
        self.rules.lock().unwrap().clone()
    }

    pub fn read_snapshot(&self) -> ReadSet {
        self.read.lock().unwrap().clone()
    }

    /// 切换单个文件的已读状态，返回切换后的新状态。
    ///
    /// 先改内存再落盘：落盘失败也不回滚 —— 用户看到的状态必须与点击一致，
    /// 磁盘问题在下次操作时自然会再写一次。返回 `Ok` 与否只影响日志。
    pub fn toggle_read(&self, path: &str) -> AppResult<bool> {
        let now_read = {
            let mut set = self.read.lock().unwrap();
            if set.contains(path) {
                set.remove(path);
                false
            } else {
                set.insert(path.to_string());
                true
            }
        };
        let snapshot = self.read_snapshot();
        store::save_read(&self.paths, &snapshot)?;
        Ok(now_read)
    }

    /// 整组一次性设置已读状态（右键菜单对多选/单文件都用这一条路径）
    pub fn set_read(&self, paths: &[String], read: bool) -> AppResult<()> {
        {
            let mut set = self.read.lock().unwrap();
            for p in paths {
                if read {
                    set.insert(p.clone());
                } else {
                    set.remove(p);
                }
            }
        }
        let snapshot = self.read_snapshot();
        store::save_read(&self.paths, &snapshot)
    }

    /// 清空全部已读标记（设置页用）
    pub fn clear_read(&self) -> AppResult<usize> {
        let count = {
            let mut set = self.read.lock().unwrap();
            let n = set.len();
            set.clear();
            n
        };
        let snapshot = self.read_snapshot();
        store::save_read(&self.paths, &snapshot)?;
        Ok(count)
    }

    pub fn data_dir(&self) -> PathBuf {
        self.paths.root.clone()
    }

    /// 取库：有缓存直接返回，否则扫描。
    ///
    /// 并发调用共享同一次扫描 —— 拿到锁后先复查缓存，
    /// 等锁期间别人已经扫完的话直接用结果，不重复扫。
    pub async fn library(&self) -> Library {
        let cached = { self.library.lock().unwrap().clone() };
        if let Some(lib) = cached {
            return lib;
        }

        let _guard = self.scan_lock.lock().await;

        let cached = { self.library.lock().unwrap().clone() };
        if let Some(lib) = cached {
            return lib;
        }

        let lib = scanner::scan_library(&self.settings_snapshot());
        *self.library.lock().unwrap() = Some(lib.clone());
        lib
    }

    /// 强制重扫
    pub async fn rescan(&self) -> Library {
        let _guard = self.scan_lock.lock().await;
        let lib = scanner::scan_library(&self.settings_snapshot());
        *self.library.lock().unwrap() = Some(lib.clone());
        lib
    }

    /// 设置变更后让缓存的库失效（根目录可能被改了）
    pub fn invalidate_library(&self) {
        *self.library.lock().unwrap() = None;
    }

    /// 构造缩略图生成上下文。
    ///
    /// ffmpeg 路径在这里先解析好，避免阻塞线程里再去抢 `meta` 锁。
    pub fn thumb_ctx(&self) -> ThumbCtx {
        let settings = self.settings_snapshot();
        let data_dir = self.paths.root.clone();
        let (ffmpeg, ffprobe) = {
            let mut cache = self.meta.lock().unwrap();
            (
                crate::metadata::ffmpeg_exe(&mut cache, &settings, &data_dir),
                crate::metadata::ffprobe_exe(&mut cache, &settings, &data_dir),
            )
        };
        ThumbCtx {
            settings,
            data_dir,
            ffmpeg,
            ffprobe,
        }
    }
}
