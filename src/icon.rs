use eframe::egui::ColorImage;
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HDC, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_FLAGS_AND_ATTRIBUTES};
use windows::Win32::UI::Shell::{
    SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

pub fn get_windows_icon(is_dir: bool, ext: &str) -> Option<HICON> {
    let dummy_path = if is_dir {
        "folder".to_string()
    } else if ext.is_empty() || ext == "__file__" {
        "dummy.file".to_string()
    } else if ext.starts_with('.') {
        format!("dummy{}", ext)
    } else {
        format!("dummy.{}", ext)
    };

    let wide: Vec<u16> = dummy_path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut sfi = SHFILEINFOW::default();
    let attrs = if is_dir {
        FILE_ATTRIBUTE_DIRECTORY.0
    } else {
        0x00000080 // FILE_ATTRIBUTE_NORMAL
    };

    let flags = SHGFI_ICON | SHGFI_SMALLICON | SHGFI_USEFILEATTRIBUTES;

    unsafe {
        let res = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(attrs),
            Some(&mut sfi as *mut _ as *mut _),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            flags,
        );
        if res != 0 && !sfi.hIcon.is_invalid() {
            Some(sfi.hIcon)
        } else {
            None
        }
    }
}

pub fn hicon_to_color_image(hicon: HICON, width: usize, height: usize) -> Option<ColorImage> {
    unsafe {
        let mut icon_info = ICONINFO::default();
        if GetIconInfo(hicon, &mut icon_info).is_err() {
            let _ = DestroyIcon(hicon);
            return None;
        }

        let hdc = CreateCompatibleDC(HDC(std::ptr::null_mut()));
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut buffer = vec![0u8; width * height * 4];
        let lines = GetDIBits(
            hdc,
            icon_info.hbmColor,
            0,
            height as u32,
            Some(buffer.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = DeleteDC(hdc);
        let _ = DeleteObject(HGDIOBJ(icon_info.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(icon_info.hbmMask.0));
        let _ = DestroyIcon(hicon);

        if lines == 0 {
            return None;
        }

        for chunk in buffer.chunks_exact_mut(4) {
            let b = chunk[0];
            let r = chunk[2];
            chunk[0] = r;
            chunk[2] = b;
        }
        let has_alpha = buffer.iter().skip(3).step_by(4).any(|&a| a > 0);
        if !has_alpha {
            for chunk in buffer.chunks_exact_mut(4) {
                chunk[3] = 255;
            }
        }

        Some(ColorImage::from_rgba_unmultiplied(
            [width, height],
            &buffer,
        ))
    }
}

pub fn get_fallback_icon(is_dir: bool) -> ColorImage {
    let mut pixels = vec![0u8; 16 * 16 * 4];
    if is_dir {
        // Classic Windows yellow folder
        for y in 0..16 {
            for x in 0..16 {
                let idx = (y * 16 + x) * 4;
                if y >= 3 && y <= 14 && x >= 1 && x <= 14 {
                    if y <= 5 && x <= 6 {
                        // Folder tab
                        pixels[idx] = 232;
                        pixels[idx + 1] = 162;
                        pixels[idx + 2] = 0;
                        pixels[idx + 3] = 255;
                    } else if y >= 5 {
                        // Folder body
                        pixels[idx] = 255;
                        pixels[idx + 1] = 200;
                        pixels[idx + 2] = 59;
                        pixels[idx + 3] = 255;
                    }
                }
            }
        }
    } else {
        // Classic Windows file page
        for y in 0..16 {
            for x in 0..16 {
                let idx = (y * 16 + x) * 4;
                if y >= 1 && y <= 14 && x >= 3 && x <= 12 {
                    if y == 1 || y == 14 || x == 3 || x == 12 {
                        pixels[idx] = 160;
                        pixels[idx + 1] = 160;
                        pixels[idx + 2] = 160;
                        pixels[idx + 3] = 255;
                    } else {
                        pixels[idx] = 245;
                        pixels[idx + 1] = 245;
                        pixels[idx + 2] = 245;
                        pixels[idx + 3] = 255;
                    }
                }
            }
        }
    }
    ColorImage::from_rgba_unmultiplied([16, 16], &pixels)
}
