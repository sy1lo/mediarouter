//! 缩略图 MIME 嗅探。
//!
//! 与旧版 `sniffMime` 完全一致：缩略图服务协议不信任扩展名，
//! 直接读魔数决定 Content-Type。

pub fn sniff_mime(buf: &[u8]) -> &'static str {
    // JPEG: FF D8 FF
    if buf.len() > 2 && buf[0] == 0xFF && buf[1] == 0xD8 && buf[2] == 0xFF {
        return "image/jpeg";
    }
    // PNG: 89 50 4E 47
    if buf.len() > 7 && buf[0] == 0x89 && buf[1] == 0x50 && buf[2] == 0x4E && buf[3] == 0x47 {
        return "image/png";
    }
    // GIF: 47 49 46
    if buf.len() > 5 && buf[0] == 0x47 && buf[1] == 0x49 && buf[2] == 0x46 {
        return "image/gif";
    }
    // WEBP: RIFF....WEBP
    if buf.len() > 11
        && buf[0] == 0x52
        && buf[1] == 0x49
        && buf[2] == 0x46
        && buf[3] == 0x46
        && buf[8] == 0x57
        && buf[9] == 0x45
        && buf[10] == 0x42
        && buf[11] == 0x50
    {
        return "image/webp";
    }
    // BMP: 42 4D
    if buf.len() > 1 && buf[0] == 0x42 && buf[1] == 0x4D {
        return "image/bmp";
    }
    "image/png"
}
