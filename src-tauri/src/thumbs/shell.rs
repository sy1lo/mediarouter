//! 系统缩略图 —— 用纯 Rust COM 调 Shell 的 `IShellItemImageFactory`。
//!
//! 这是对旧版最实质的一次替换。旧版 `shell-thumb.ps1` 的做法是：
//! 起一个 `powershell.exe`（冷启动 300~800ms）→ `Add-Type` 现场把一段 C# 源码
//! 反射编译成 DLL（首次还要落盘缓存）→ 再 P/Invoke 调同样的 Shell 接口
//! → GDI+ 存 PNG。
//!
//! 现在的做法是直接调 COM：没有子进程、没有反射编译、没有 DLL 落盘。
//! 代价是这段代码无法在纯逻辑层验证，必须靠真机跑。

use std::path::Path;

/// 取系统缩略图并写成 PNG。返回 true 表示 `out` 已写出。
#[cfg(windows)]
pub fn system_thumbnail(src: &Path, out: &Path, size: u32) -> bool {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

    // 生成跑在 spawn_blocking 的工作线程上，需要自己初始化 COM 单元。
    // 若该线程已初始化过（返回 RPC_E_CHANGED_MODE 之类），不要配对 CoUninitialize。
    let needs_uninit = unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .is_ok()
    };

    let ok = unsafe { try_system_thumbnail(src, out, size) };

    if needs_uninit {
        unsafe { CoUninitialize() };
    }
    ok
}

#[cfg(not(windows))]
pub fn system_thumbnail(_src: &Path, _out: &Path, _size: u32) -> bool {
    false
}

#[cfg(windows)]
unsafe fn try_system_thumbnail(src: &Path, out: &Path, size: u32) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::DeleteObject;
    use windows::Win32::System::Com::IBindCtx;
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_BIGGERSIZEOK,
        SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT, SIIGBF_THUMBNAILONLY,
    };

    let wide: Vec<u16> = src
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // 签名是三个泛型参数：<P0: Param<PCWSTR>, P1: Param<IBindCtx>, T: Interface>。
    // 第二个参数要显式写成 Option<&IBindCtx>（impl<T> Param<T> for Option<&T>），
    // 光写 None 无法推断出 P1。
    let Ok(factory) =
        SHCreateItemFromParsingName::<_, Option<&IBindCtx>, IShellItemImageFactory>(
            PCWSTR(wide.as_ptr()),
            None,
        )
    else {
        return false;
    };

    // 四档降级。**第一档刻意不放 BIGGERSIZEOK**：
    // 带上它等于允许 Shell 返回超出请求尺寸的图，实测会拿到原始分辨率的
    // 无损 PNG（单张 875KB，比正确缩放后大 50 倍）。先要精确尺寸，拿不到再放宽；
    // 即便放宽了，下面 hbitmap_to_image 也会再把尺寸裁回去。
    let cascade: [SIIGBF; 4] = [
        SIIGBF(SIIGBF_THUMBNAILONLY.0),
        SIIGBF(SIIGBF_BIGGERSIZEOK.0 | SIIGBF_THUMBNAILONLY.0),
        SIIGBF(SIIGBF_RESIZETOFIT.0),
        SIIGBF(SIIGBF_ICONONLY.0),
    ];

    for flags in cascade {
        // 注意：windows-rs 把 COM 的 [out] 参数改写成了返回值，
        // 所以这里不需要自己传 &mut HBITMAP：
        //   GetImage(&self, size: SIZE, flags: SIIGBF) -> Result<HBITMAP>
        let Ok(hbmp) = factory.GetImage(
            SIZE {
                cx: size as i32,
                cy: size as i32,
            },
            flags,
        ) else {
            continue;
        };
        if hbmp.is_invalid() {
            continue;
        }

        let ok = hbitmap_to_image(hbmp, out, size);
        let _ = DeleteObject(hbmp.into());
        if ok {
            return true;
        }
    }

    false
}

/// HBITMAP → 落盘图片文件。
///
/// 输出策略：
/// - **尺寸封顶**：超过 `max_side` 就等比缩回去。Shell 允许返回超尺寸的图，
///   不封顶会写出原始分辨率的巨图。
/// - **不透明用 JPEG**：Shell 产物绝大多数是视频帧/封面，不带透明通道；
///   同样内容 JPEG 比 PNG 小一个量级，对「一屏几十张图」的加载速度影响很大。
///   真有透明通道（图标类）才落 PNG。
#[cfg(windows)]
unsafe fn hbitmap_to_image(
    hbmp: windows::Win32::Graphics::Gdi::HBITMAP,
    out: &Path,
    max_side: u32,
) -> bool {
    use std::ffi::c_void;
    use windows::Win32::Graphics::Gdi::{
        GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS,
    };

    // 先把位图属性读出来
    let mut bm = BITMAP::default();
    let got = GetObjectW(
        hbmp.into(),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bm as *mut _ as *mut c_void),
    );
    if got == 0 {
        return false;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight);
    if w <= 0 || h <= 0 {
        return false;
    }

    // 统一取成 32bpp 自顶向下的 BGRA，省去按位深分支
    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h, // 负数 = top-down
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let mut bgra = vec![0u8; (w as usize) * (h as usize) * 4];

    let hdc = GetDC(None);
    let scanlines = GetDIBits(
        hdc,
        hbmp,
        0,
        h as u32,
        Some(bgra.as_mut_ptr() as *mut c_void),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    ReleaseDC(None, hdc);

    if scanlines == 0 {
        return false;
    }

    // BGRA → RGBA
    let mut any_alpha = false;
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
        if px[3] != 0 {
            any_alpha = true;
        }
    }
    // Shell 返回的位图经常整幅 alpha=0（明明是不透明图）。
    // 只有在「一个非零 alpha 都没有」时才强制不透明，避免误伤真有透明通道的图标。
    if !any_alpha {
        for px in bgra.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }

    let Some(src_img) = image::RgbaImage::from_raw(w as u32, h as u32, bgra) else {
        return false;
    };

    // 尺寸封顶
    let img = if w.max(h) as u32 > max_side {
        let scale = max_side as f32 / w.max(h) as f32;
        let nw = ((w as f32 * scale).round() as u32).max(1);
        let nh = ((h as f32 * scale).round() as u32).max(1);
        image::imageops::resize(&src_img, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        src_img
    };
    let (fw, fh) = (img.width(), img.height());
    if fw == 0 || fh == 0 {
        return false;
    }

    // write_image 是 ImageEncoder trait 的方法，必须把 trait 引入作用域
    use image::ImageEncoder;

    let mut encoded: Vec<u8> = Vec::new();

    if any_alpha {
        if image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(
                img.as_raw(),
                fw,
                fh,
                image::ExtendedColorType::Rgba8,
            )
            .is_err()
        {
            return false;
        }
    } else {
        // 收成 RGB —— JPEG 不接受 alpha 通道
        let mut rgb = Vec::with_capacity((fw as usize) * (fh as usize) * 3);
        for px in img.as_raw().chunks_exact(4) {
            rgb.extend_from_slice(&px[..3]);
        }
        if image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 82)
            .write_image(&rgb, fw, fh, image::ExtendedColorType::Rgb8)
            .is_err()
        {
            return false;
        }
    }

    std::fs::write(out, &encoded).is_ok()
}
