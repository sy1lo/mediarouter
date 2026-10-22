//! 媒体库扫描。
//!
//! 语义与旧版 `electron/scanner.js` 完全对齐：
//! - 每个分类有独立的扩展名白名单
//! - 根目录下的一级子目录各自成为一个「作品集」，递归深度上限 6
//! - 根目录下散落的合格文件归入一个 `loose` 作品集
//! - 用户手工挂载的额外作品集带 `extra` 标记
//!
//! **排序在这里做**（而不是像上一版那样交给前端）：
//! 前端当时用 `Intl.Collator('zh-CN', {numeric: true})`，那是 ICU 的中文规则，
//! **不是资源管理器的规则** —— ICU 把汉字排在拉丁字母之前，导致同一个文件夹
//! 在两个窗口里的顺序对不上（用户实际反馈过）。
//! 现在统一用 `crate::sort`，它直接调用资源管理器的排序函数 `StrCmpLogicalW`，
//! 两边不可能不一致。详见 `sort.rs` 的模块说明。
//!
//! 排序是**就地做在扫描结果上**的，所以缓存里存的就已经是排好序的，
//! 前端拿到即用、不再需要二次排序（否则会用错误的规则把顺序改回去）。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::model::{CategoryData, Collection, Library, MediaFile, Settings, CATEGORIES};
use crate::platform::now_ms;
use crate::sort;

/// 递归深度上限（根的一级子目录算第 0 层）
const MAX_DEPTH: usize = 6;

/// 各分类的扩展名白名单
fn exts_for(category: &str) -> &'static [&'static str] {
    match category {
        "动漫" => &[
            "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "ts", "rmvb",
        ],
        "漫画" => &[
            "zip", "cbz", "cbr", "cbt", "pdf", "epub", "mobi", "azw3", "png", "jpg", "jpeg",
            "webp", "gif",
        ],
        "小说" => &["epub", "txt", "mobi", "azw3", "pdf", "docx", "doc", "md"],
        // 图片分类只收位图。刻意不收 svg：WebView 能渲染，但 Windows 缩略图与
        // 尺寸探测都对它无效，混进来只会得到一堆占位块。
        "图片" => &[
            "jpg", "jpeg", "jfif", "png", "gif", "webp", "bmp", "tif", "tiff", "avif", "ico",
            "heic", "heif",
        ],
        _ => &[],
    }
}

/// 该分类的文件是否需要附带「原始宽高」。
///
/// 只有图片分类要 —— 它的三级菜单是平铺填满的 justify 布局，
/// 必须提前知道每张图的宽高比（见 `crate::imagedims`）。
/// 其他分类的卡片是固定比例，多读一次文件头纯属浪费。
fn needs_image_size(category: &str) -> bool {
    category == "图片"
}

/// 取一个文件的扩展名（小写、不含点）
fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// 组装 MediaFile；文件不可读时返回 None
fn file_info(path: &Path, category: &str) -> Option<MediaFile> {
    let md = fs::metadata(path).ok()?;
    if !md.is_file() {
        return None;
    }
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0);
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let (width, height) = if needs_image_size(category) {
        match crate::imagedims::read_size(path) {
            Some((w, h)) => (Some(w), Some(h)),
            // 解析不出来（少见格式/损坏文件）就留空 —— 前端会用兜底比例摆位，
            // 比在这里瞎猜一个尺寸好
            None => (None, None),
        }
    } else {
        (None, None)
    };

    Some(MediaFile {
        name,
        path: path.to_string_lossy().into_owned(),
        ext: ext_of(path),
        size: md.len(),
        mtime,
        category: Some(category.to_string()),
        width,
        height,
    })
}

fn total_size(files: &[MediaFile]) -> u64 {
    files.iter().map(|f| f.size).sum()
}

/// 递归收集目录下的合格文件
fn list_files(dir: &Path, exts: &[&str], category: &str, out: &mut Vec<MediaFile>, depth: usize) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // 跳过隐藏项，与旧版一致
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if depth < MAX_DEPTH {
                list_files(&path, exts, category, out, depth + 1);
            }
        } else if ft.is_file() {
            if !exts.contains(&ext_of(&path).as_str()) {
                continue;
            }
            if let Some(f) = file_info(&path, category) {
                out.push(f);
            }
        }
    }
}

/// 扫描一个根目录，产出该根下的所有作品集
fn scan_root(root: &Path, exts: &[&str], category: &str) -> Vec<Collection> {
    let mut collections = Vec::new();
    let mut loose = Vec::new();

    let Ok(entries) = fs::read_dir(root) else {
        return collections;
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };

        if ft.is_dir() {
            let mut files = Vec::new();
            list_files(&path, exts, category, &mut files, 0);
            if files.is_empty() {
                continue;
            }
            collections.push(Collection {
                id: format!("dir:{}", path.to_string_lossy()),
                name,
                path: path.to_string_lossy().into_owned(),
                count: files.len(),
                total_size: total_size(&files),
                files,
                loose: false,
                extra: false,
            });
        } else if ft.is_file() {
            if !exts.contains(&ext_of(&path).as_str()) {
                continue;
            }
            if let Some(f) = file_info(&path, category) {
                loose.push(f);
            }
        }
    }

    if !loose.is_empty() {
        collections.push(Collection {
            id: format!("root:{}", root.to_string_lossy()),
            name: root
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.to_string_lossy().into_owned()),
            path: root.to_string_lossy().into_owned(),
            count: loose.len(),
            total_size: total_size(&loose),
            files: loose,
            loose: true,
            extra: false,
        });
    }

    collections
}

/// 全量扫描。纯同步实现，调用方负责放到异步上下文里跑。
pub fn scan_library(settings: &Settings) -> Library {
    let mut categories: BTreeMap<String, CategoryData> = BTreeMap::new();
    for cat in CATEGORIES {
        let exts = exts_for(cat);
        let roots = settings.categories.get(cat).cloned().unwrap_or_default();
        let mut collections: Vec<Collection> = Vec::new();

        for root in &roots {
            collections.extend(scan_root(Path::new(root), exts, cat));
        }

        if let Some(extras) = settings.extra_collections.get(cat) {
            for ex in extras {
                if ex.path.is_empty() {
                    continue;
                }
                let path = PathBuf::from(&ex.path);
                if !path.exists() {
                    continue;
                }
                let mut files = Vec::new();
                list_files(&path, exts, cat, &mut files, 0);
                collections.push(Collection {
                    id: format!("extra:{}", ex.path),
                    name: if ex.name.is_empty() {
                        path.file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| ex.path.clone())
                    } else {
                        ex.name.clone()
                    },
                    path: ex.path.clone(),
                    count: files.len(),
                    total_size: total_size(&files),
                    files,
                    loose: false,
                    extra: true,
                });
            }
        }

        categories.insert(
            cat.to_string(),
            CategoryData {
                roots,
                collections,
            },
        );
    }

    let mut lib = Library {
        categories,
        scanned_at: now_ms(),
    };
    sort_library(&mut lib);
    lib
}

/// 就地排序：每个作品集内的文件、以及作品集之间。
///
/// 统一在**最后一步**做，而不是在各个构造点分别排 ——
/// 作品集有三个来源（根目录子目录、`loose` 散件、手工挂载的 `extra`），
/// 分散去排迟早会漏掉一个（旧版就是这样：只有 `loose` 排过序）。
fn sort_library(lib: &mut Library) {
    for data in lib.categories.values_mut() {
        for col in data.collections.iter_mut() {
            sort::sort_by_name(&mut col.files, |f| f.name.clone());
        }
        sort::sort_by_name(&mut data.collections, |c| c.name.clone());
    }
}

/// 给日志用的分类汇总
pub fn summarize(lib: &Library) -> String {
    let mut parts: Vec<String> = Vec::new();
    for cat in CATEGORIES {
        if let Some(data) = lib.categories.get(cat) {
            let n: usize = data.collections.iter().map(|c| c.count).sum();
            parts.push(format!("{cat}={n}"));
        }
    }
    parts.join(", ")
}
