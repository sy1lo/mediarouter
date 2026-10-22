//! 图片原始宽高探测 —— 只读文件头，不解码像素。
//!
//! ## 为什么需要它
//!
//! 图片分类的三级菜单是「平铺填满」的 justify 布局：每行按容器的实际宽度
//! 把这一行的图等比放到**刚好铺满**，所以每张图的宽高比是布局的输入。
//!
//! 如果拿不到宽高比，就只能先按一个占位比例排一遍、等图片加载完再重算 ——
//! 而「重算」就是一次全量重排，表现是**打开文件夹后版面整体跳一下**。
//! 这个项目已经在字体继承问题上栽过一次「界面跳一下」，不再重蹈。
//!
//! 所以在扫描阶段就把宽高读出来随 `MediaFile` 下发，布局一次算准、零重排。
//!
//! ## 成本
//!
//! 一次 `open` + 读 16KB（JPEG 的 SOF 段极少超出这个范围；EXIF/ICC 特别大的
//! 才会触发一次 512KB 的重试）。相比「生成 512px 系统缩略图」，
//! 这点开销可以忽略 —— 何况原图浏览本来就不需要缩略图。
//!
//! ## 支持的格式
//!
//! PNG / JPEG / GIF / BMP / WebP（VP8、VP8L、VP8X）。
//! 其余返回 `None`，前端用兜底比例摆位 —— 宁可比例不准，也不给一个错的值。

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// 首次读取的字节数
const HEAD_BYTES: usize = 16 * 1024;
/// 首次没找到 SOF 时的重试读取量（EXIF/ICC 很大的 JPEG）
const RETRY_BYTES: usize = 512 * 1024;
/// 边长上限：超过这个值一定是我们解析错了，不是真有这么大的图
const MAX_EDGE: u32 = 100_000;

/// 读一张图的原始宽高；解析不出来返回 `None`
pub fn read_size(path: &Path) -> Option<(u32, u32)> {
    let mut head = read_head(path, HEAD_BYTES)?;
    if let Some(wh) = parse(&head) {
        return Some(wh);
    }

    // 只有 JPEG 值得重试 —— 它是唯一一个「尺寸位置不固定、要顺着段链找」的格式，
    // 其余格式的尺寸都在固定偏移上，第一次没解出来就是真的解不出来。
    // 另外只有真的读满了 HEAD_BYTES 才说明可能是"还没读到"，否则是文件本身就短。
    if head.len() >= HEAD_BYTES && is_jpeg(&head) {
        head = read_head(path, RETRY_BYTES)?;
        return parse(&head);
    }
    None
}

/// 读文件开头至多 `max` 字节（短读合法，读到多少算多少）
fn read_head(path: &Path, max: usize) -> Option<Vec<u8>> {
    let mut f = File::open(path).ok()?;

    // 先看文件大小，避免给小文件白分配 512KB
    let len = f.metadata().ok()?.len() as usize;
    let cap = max.min(len.max(1));

    let mut buf = vec![0u8; cap];
    let mut filled = 0usize;
    while filled < cap {
        match f.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    buf.truncate(filled);
    Some(buf)
}

/// 按魔数逐个试。各格式的签名互斥，顺序无关紧要，只是把常见格式放前面。
fn parse(b: &[u8]) -> Option<(u32, u32)> {
    png(b).or_else(|| jpeg(b)).or_else(|| gif(b)).or_else(|| bmp(b)).or_else(|| webp(b))
}

fn is_jpeg(b: &[u8]) -> bool {
    b.len() >= 2 && b[0] == 0xFF && b[1] == 0xD8
}

// ---------------------------------------------------------------------------
// 各格式
// ---------------------------------------------------------------------------

/// PNG：8 字节签名 + 4 字节块长 + "IHDR" + 宽(4, BE) + 高(4, BE)
fn png(b: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if b.get(..8)? != SIG {
        return None;
    }
    if b.get(12..16)? != b"IHDR" {
        return None;
    }
    let w = be32(b.get(16..20)?)?;
    let h = be32(b.get(20..24)?)?;
    sane(w, h)
}

/// JPEG：从 FF D8 开始顺着段链走，找到 SOF 段即为尺寸。
///
/// SOF 的几种变体（0xC0~0xCF）里要排除 0xC4(DHT)、0xC8(JPG)、0xCC(DAC) ——
/// 它们同样落在这个区间但结构完全不同，误读会得到垃圾数值。
fn jpeg(b: &[u8]) -> Option<(u32, u32)> {
    if !is_jpeg(b) {
        return None;
    }
    let mut i = 2usize;
    while i + 3 < b.len() {
        if b[i] != 0xFF {
            // 没对齐到标记边界，向前蹭一格重新对齐
            i += 1;
            continue;
        }
        let m = b[i + 1];
        if m == 0xFF {
            // 填充字节，跳过当前这个 FF
            i += 1;
            continue;
        }
        // 无长度字段的标记：TEM(0x01) 与 RSTn/SOI/EOI(0xD0~0xD8)
        if m == 0x01 || (0xD0..=0xD8).contains(&m) {
            i += 2;
            continue;
        }
        let seg = be16(b.get(i + 2..i + 4)?)? as usize;
        if seg < 2 {
            return None;
        }
        let is_sof = (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC;
        if is_sof {
            // SOF 段：长度(2) 精度(1) 高(2) 宽(2)
            let h = be16(b.get(i + 5..i + 7)?)?;
            let w = be16(b.get(i + 7..i + 9)?)?;
            return sane(w as u32, h as u32);
        }
        i += 2 + seg;
    }
    None
}

/// GIF：`GIF87a`/`GIF89a` + 宽(2, LE) + 高(2, LE)
fn gif(b: &[u8]) -> Option<(u32, u32)> {
    let sig = b.get(..6)?;
    if sig != b"GIF87a" && sig != b"GIF89a" {
        return None;
    }
    let w = le16(b.get(6..8)?)? as u32;
    let h = le16(b.get(8..10)?)? as u32;
    sane(w, h)
}

/// BMP：`BM` + 文件头(12) + DIB 头。
///
/// DIB 头有两种：12 = BITMAPCOREHEADER（尺寸是 u16），40+ = INFOHEADER 系（i32）。
/// 后者在自顶向下位图里高度是**负数**，取绝对值。
fn bmp(b: &[u8]) -> Option<(u32, u32)> {
    if b.get(..2)? != b"BM" {
        return None;
    }
    let dib = le32(b.get(14..18)?)?;
    if dib == 12 {
        let w = le16(b.get(18..20)?)? as u32;
        let h = le16(b.get(20..22)?)? as u32;
        return sane(w, h);
    }
    if dib >= 40 {
        let w = i32::from_le_bytes(b.get(18..22)?.try_into().ok()?);
        let h = i32::from_le_bytes(b.get(22..26)?.try_into().ok()?);
        return sane(w.unsigned_abs(), h.unsigned_abs());
    }
    None
}

/// WebP：`RIFF` + 长度 + `WEBP` + 块头，块体按 VP8 / VP8L / VP8X 三种排布。
fn webp(b: &[u8]) -> Option<(u32, u32)> {
    if b.get(..4)? != b"RIFF" || b.get(8..12)? != b"WEBP" {
        return None;
    }
    match b.get(12..16)? {
        // 有损：3 字节帧标签 + 同步码 9D 01 2A，之后是 14 位的宽、高（各占 2 字节，只用低 14 位）
        b"VP8 " => {
            if b.get(23..26)? != [0x9D, 0x01, 0x2A] {
                return None;
            }
            let w = le16(b.get(26..28)?)? as u32 & 0x3FFF;
            let h = le16(b.get(28..30)?)? as u32 & 0x3FFF;
            sane(w, h)
        }
        // 无损：0x2F 之后 4 字节里藏着 14 位的(宽-1) 与 14 位的(高-1)
        b"VP8L" => {
            if *b.get(20)? != 0x2F {
                return None;
            }
            let bits = le32(b.get(21..25)?)?;
            let w = (bits & 0x3FFF) + 1;
            let h = ((bits >> 14) & 0x3FFF) + 1;
            sane(w, h)
        }
        // 扩展：3 字节保留位之后，24 位的(宽-1) 与 24 位的(高-1)
        b"VP8X" => {
            let w = le24(b.get(24..27)?)? + 1;
            let h = le24(b.get(27..30)?)? + 1;
            sane(w, h)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------

/// 过滤不合理值。解析错位时很容易读出天文数字，与其让布局算出个荒谬的宽度，
/// 不如返回 `None` 让前端走兜底比例。
fn sane(w: u32, h: u32) -> Option<(u32, u32)> {
    if w == 0 || h == 0 || w > MAX_EDGE || h > MAX_EDGE {
        None
    } else {
        Some((w, h))
    }
}

fn be16(b: &[u8]) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(..2)?.try_into().ok()?))
}

fn be32(b: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(..4)?.try_into().ok()?))
}

fn le16(b: &[u8]) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(..2)?.try_into().ok()?))
}

fn le32(b: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(..4)?.try_into().ok()?))
}

fn le24(b: &[u8]) -> Option<u32> {
    let s = b.get(..3)?;
    Some(u32::from(s[0]) | (u32::from(s[1]) << 8) | (u32::from(s[2]) << 16))
}

// ---------------------------------------------------------------------------
// 用最小可用文件头验证解析 —— 不需要真的图片文件
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::parse;

    /// PNG：签名 + IHDR 块，宽 800 高 600
    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        v.extend_from_slice(&13u32.to_be_bytes()); // IHDR 长度
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v
    }

    /// JPEG：SOI + 一段 APP0 + SOF0，宽 1920 高 1080
    fn jpeg_bytes(w: u16, h: u16) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8];
        // APP0（长度含自身 2 字节）
        v.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        v.extend_from_slice(b"JFIF\0");
        v.extend_from_slice(&[0u8; 9]);
        // SOF0：长度 17，精度 8，高，宽，3 个分量
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&[0x03, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        v
    }

    fn gif_bytes(w: u16, h: u16) -> Vec<u8> {
        let mut v = b"GIF89a".to_vec();
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v
    }

    fn bmp_bytes(w: i32, h: i32) -> Vec<u8> {
        let mut v = b"BM".to_vec();
        v.extend_from_slice(&[0u8; 12]); // 文件头余下部分
        v.extend_from_slice(&40u32.to_le_bytes()); // BITMAPINFOHEADER
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v
    }

    /// VP8X（扩展格式）最容易构造：明文 24 位宽高
    fn webp_vp8x_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"RIFF".to_vec();
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(b"WEBP");
        v.extend_from_slice(b"VP8X");
        v.extend_from_slice(&10u32.to_le_bytes());
        v.extend_from_slice(&[0u8; 4]); // flags + reserved
        let wz = w - 1;
        let hz = h - 1;
        v.extend_from_slice(&[wz as u8, (wz >> 8) as u8, (wz >> 16) as u8]);
        v.extend_from_slice(&[hz as u8, (hz >> 8) as u8, (hz >> 16) as u8]);
        v
    }

    #[test]
    fn parses_png() {
        assert_eq!(parse(&png_bytes(800, 600)), Some((800, 600)));
        assert_eq!(parse(&png_bytes(1, 1)), Some((1, 1)));
    }

    #[test]
    fn parses_jpeg_past_other_segments() {
        // 尺寸在 APP0 之后才出现，段链必须走对
        assert_eq!(parse(&jpeg_bytes(1920, 1080)), Some((1920, 1080)));
        assert_eq!(parse(&jpeg_bytes(3, 7)), Some((3, 7)));
    }

    #[test]
    fn jpeg_ignores_non_sof_markers_in_same_range() {
        // 0xC4(DHT) / 0xC8(JPG) / 0xCC(DAC) 与 SOF 同区间，不能被当成尺寸
        let mut v = vec![0xFFu8, 0xD8];
        v.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x04, 0xAA, 0xBB]); // DHT
        v.extend_from_slice(&[0xFF, 0xCC, 0x00, 0x04, 0xCC, 0xDD]); // DAC
        v.extend_from_slice(&[0xFF, 0xC2, 0x00, 0x11, 0x08]); // SOF2（渐进式）
        v.extend_from_slice(&321u16.to_be_bytes());
        v.extend_from_slice(&654u16.to_be_bytes());
        v.extend_from_slice(&[0x03, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        assert_eq!(parse(&v), Some((654, 321)));
    }

    #[test]
    fn jpeg_skips_fill_bytes() {
        // FF FF 填充后紧跟真正的标记
        let mut v = vec![0xFFu8, 0xD8, 0xFF, 0xFF];
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        v.extend_from_slice(&200u16.to_be_bytes());
        v.extend_from_slice(&300u16.to_be_bytes());
        v.extend_from_slice(&[0x03, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        assert_eq!(parse(&v), Some((300, 200)));
    }

    #[test]
    fn parses_gif_and_bmp() {
        assert_eq!(parse(&gif_bytes(640, 480)), Some((640, 480)));
        assert_eq!(parse(&bmp_bytes(1024, 768)), Some((1024, 768)));
        // 自顶向下位图的高度是负数
        assert_eq!(parse(&bmp_bytes(1024, -768)), Some((1024, 768)));
    }

    #[test]
    fn parses_webp_vp8x() {
        assert_eq!(parse(&webp_vp8x_bytes(1280, 720)), Some((1280, 720)));
    }

    #[test]
    fn rejects_garbage_and_absurd_sizes() {
        assert_eq!(parse(&[]), None);
        assert_eq!(parse(&[0u8; 32]), None);
        assert_eq!(parse(b"not an image at all, really"), None);
        // 解析错位读出的天文数字要被挡掉，不能把布局算崩
        assert_eq!(parse(&png_bytes(0, 600)), None);
        assert_eq!(parse(&png_bytes(800, 0)), None);
        assert_eq!(parse(&png_bytes(999_999, 1)), None);
        // 截断的 PNG（只有签名）不该 panic
        assert_eq!(parse(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]), None);
        // 截断的 JPEG 段头
        assert_eq!(parse(&[0xFF, 0xD8, 0xFF, 0xC0, 0x00]), None);
    }
}
