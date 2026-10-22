//! epub 封面提取 —— 六级级联。
//!
//! 这是整个缩略图管线里最容易出错的一段，逻辑严格照搬旧版
//! `electron/thumbs.js` 的 `extractEpubCover`，包括一个**看起来像 bug 但必须保留**
//! 的细节：
//!
//! 候选封面在旧版里带 `score` 字段（封面属性 100 / meta 引用 95 / id=cover 90），
//! 但这个 score **从未参与排序** —— 旧版是把候选按「推入顺序」依次试的。
//! 所以真实优先级是：`properties="cover-image"` > `id="cover"` > `meta name="cover"`。
//! 这里必须复刻同样的顺序，不能"顺手修成按分数排序"。
//!
//! 级联顺序：
//!   1. META-INF/container.xml → rootfile 的 full-path → OPF
//!   2. OPF 里 `properties="…cover-image…"` 的 item
//!   3. OPF 里 `id="cover"` 的 item
//!   4. OPF 里 `<meta name="cover" content="X">` 指向的 item
//!   5. 沿 spine 顺序找书页 HTML 里第一张 `<img src>`
//!   6. 压缩包里第一个文件名含 cover/front/title/0001/page0/封面/扉页 的图片
//!   （全部失败 → 由调用方兜底取包内第一张图）

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use zip::ZipArchive;

use super::{first_image_index, is_image, read_text, write_entry_to, write_index_to, MAX_ZIP_SIZE};

// ---------------------------------------------------------------------------
// 正则（编译一次，进程内复用）
// ---------------------------------------------------------------------------

macro_rules! regex_once {
    ($fn_name:ident, $pat:expr) => {
        fn $fn_name() -> &'static Regex {
            static CELL: OnceLock<Regex> = OnceLock::new();
            CELL.get_or_init(|| Regex::new($pat).expect("内置正则编译失败"))
        }
    };
}

regex_once!(re_rootfile, r#"(?i)rootfile[^>]*full-path\s*=\s*"([^"]+)""#);
regex_once!(re_item, r"(?i)<item\b[^>]*/?>");
regex_once!(re_id_attr, r#"(?i)\bid\s*=\s*"([^"]+)""#);
regex_once!(re_cover_image_prop, r#"(?i)properties\s*=\s*"[^"]*cover-image[^"]*""#);
regex_once!(re_id_cover, r#"(?i)\bid\s*=\s*"cover""#);
regex_once!(
    re_meta_cover_a,
    r#"(?i)<meta\b[^>]*name\s*=\s*"cover"[^>]*content\s*=\s*"([^"]+)""#
);
regex_once!(
    re_meta_cover_b,
    r#"(?i)<meta\b[^>]*content\s*=\s*"([^"]+)"[^>]*name\s*=\s*"cover""#
);
regex_once!(
    re_spine,
    r#"(?i)<itemref\b[^>]*idref\s*=\s*"([^"]+)""#
);
regex_once!(
    re_img_src,
    r#"(?i)<img\b[^>]*\ssrc\s*=\s*"([^"]+)"|<img\b[^>]*\ssrc\s*=\s*'([^']+)'"#
);
regex_once!(re_href_dq, r#"(?i)href\s*=\s*"([^"]+)""#);
regex_once!(re_href_sq, r#"(?i)href\s*=\s*'([^']+)'"#);
regex_once!(
    re_cover_name,
    r"(?i)(cover|front|title|0001|001|page0|封面|扉页)"
);

// ---------------------------------------------------------------------------
// 路径工具（POSIX 语义，与 Node 的 path.posix 对齐）
// ---------------------------------------------------------------------------

/// `path.posix.dirname`
fn posix_dirname(p: &str) -> String {
    match p.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => p[..i].to_string(),
        None => ".".to_string(),
    }
}

/// `path.posix.basename`
fn posix_basename(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[i + 1..],
        None => p,
    }
}

/// `path.posix.normalize`
fn posix_normalize(p: &str) -> String {
    let absolute = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => match out.last() {
                Some(last) if *last != ".." => {
                    out.pop();
                }
                _ => {
                    if !absolute {
                        out.push("..");
                    }
                }
            },
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

/// 把 opf 里的相对 href 解析成包内条目名。
/// 拒绝 data:/http:/file:/绝对路径（与旧版一致），并做百分号解码。
fn normalize_href(base: &str, href: &str) -> Option<String> {
    let decoded = percent_encoding::percent_decode_str(href)
        .decode_utf8_lossy()
        .into_owned();

    let lower = decoded.to_lowercase();
    if lower.starts_with("data:")
        || lower.starts_with("http:")
        || lower.starts_with("https:")
        || lower.starts_with("file:")
        || decoded.starts_with('/')
    {
        return None;
    }

    let joined = if base.is_empty() || base == "." {
        decoded
    } else {
        format!("{base}/{decoded}")
    };
    Some(posix_normalize(&joined))
}

/// 最小 XML 实体解码。只用于「按原样找不到条目时再试一次」的兜底。
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#38;", "&")
}

fn item_href(tag: &str) -> Option<String> {
    re_href_dq()
        .captures(tag)
        .or_else(|| re_href_sq().captures(tag))
        .map(|c| c[1].to_string())
}

/// 写出一条 href 指向的图片；原样失败时再试实体解码版本
fn write_href(zip: &mut ZipArchive<File>, base: &str, href: &str, out: &Path) -> bool {
    if let Some(p) = normalize_href(base, href) {
        if write_entry_to(zip, &p, out) {
            return true;
        }
    }
    let decoded = decode_entities(href);
    if decoded != href {
        if let Some(p) = normalize_href(base, &decoded) {
            if write_entry_to(zip, &p, out) {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// 主流程
// ---------------------------------------------------------------------------

/// 提取 epub 封面。返回 true 表示 `out` 已写出。
pub fn extract_cover(zip_path: &str, out: &Path) -> bool {
    let Ok(file) = File::open(zip_path) else {
        return false;
    };
    let Ok(md) = file.metadata() else {
        return false;
    };
    if md.len() > MAX_ZIP_SIZE {
        return false;
    }
    let Ok(mut zip) = ZipArchive::new(file) else {
        return false;
    };

    // 第 1 级：container.xml → OPF
    if let Some(opf_path) = locate_opf(&mut zip) {
        if let Some(opf) = read_text(&mut zip, &opf_path) {
            let base = posix_dirname(&opf_path);
            if try_opf(&mut zip, &opf, &base, out) {
                return true;
            }
        }
    }

    // 终级兜底：包内第一张图
    if let Some(idx) = first_image_index(&mut zip) {
        return write_index_to(&mut zip, idx, out);
    }
    false
}

/// 从 META-INF/container.xml 里读出 OPF 的路径
fn locate_opf(zip: &mut ZipArchive<File>) -> Option<String> {
    let container = read_text(zip, "META-INF/container.xml")?;
    let caps = re_rootfile().captures(&container)?;
    // 有的包把 OPF 路径写成反斜杠
    Some(caps[1].replace('\\', "/"))
}

/// 在 OPF 里按级联顺序找封面
fn try_opf(zip: &mut ZipArchive<File>, opf: &str, base: &str, out: &Path) -> bool {
    let items: Vec<&str> = re_item().find_iter(opf).map(|m| m.as_str()).collect();

    let mut by_id: HashMap<String, &str> = HashMap::new();
    for tag in &items {
        if let Some(c) = re_id_attr().captures(tag) {
            by_id.insert(c[1].to_string(), *tag);
        }
    }

    // 候选顺序 = 旧版的推入顺序（见文件头注释，score 是死数据）
    let mut candidates: Vec<&str> = Vec::new();
    for tag in &items {
        if re_cover_image_prop().is_match(tag) {
            candidates.push(*tag);
        }
    }
    for tag in &items {
        if re_id_cover().is_match(tag) {
            candidates.push(*tag);
        }
    }
    if let Some(c) = re_meta_cover_a()
        .captures(opf)
        .or_else(|| re_meta_cover_b().captures(opf))
    {
        // 注意：regex 的 `Captures: Index<usize>` 关联类型是 `str`（不是 `&str`），
        // 所以 `c[1]` 是个 str 位置表达式，必须写成 `&c[1]` 才是 `&str`。
        // （直接在 c[1] 上调 .as_str() 会解析到未稳定的 str::as_str，
        //   .as_ref() 则会因为 str 的多个 AsRef 实现而歧义）
        if let Some(tag) = by_id.get(&c[1]) {
            candidates.push(*tag);
        }
    }

    // 第 2~4 级
    for tag in &candidates {
        if let Some(href) = item_href(tag) {
            if write_href(zip, base, &href, out) {
                return true;
            }
        }
    }

    // 第 5 级：沿 spine 找书页里第一张图
    for cap in re_spine().captures_iter(opf) {
        let Some(tag) = by_id.get(&cap[1]).copied() else {
            continue;
        };
        let Some(href) = item_href(tag) else {
            continue;
        };
        let Some(page) = normalize_href(base, &href) else {
            continue;
        };
        let Some(doc) = read_text(zip, &page) else {
            continue;
        };
        let page_base = posix_dirname(&page);
        for im in re_img_src().captures_iter(&doc) {
            let Some(src) = im.get(1).or_else(|| im.get(2)) else {
                continue;
            };
            if write_href(zip, &page_base, src.as_str(), out) {
                return true;
            }
        }
    }

    // 第 6 级：按文件名启发挑第一张"像封面"的图
    let mut hit: Option<usize> = None;
    for i in 0..zip.len() {
        let name = match zip.by_index(i) {
            Ok(e) if !e.is_dir() => e.name().to_string(),
            _ => continue,
        };
        if !is_image(&name) {
            continue;
        }
        if re_cover_name().is_match(posix_basename(&name)) {
            hit = Some(i);
            break;
        }
    }
    if let Some(i) = hit {
        return write_index_to(zip, i, out);
    }

    false
}
