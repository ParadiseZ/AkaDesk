#[derive(Debug, Default)]
pub struct WindowsCaptureBackendPlan {
    pub prefer_wgc: bool,
    pub allow_dxgi_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedBgraFrame {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub bytes: Vec<u8>,
}

#[cfg(windows)]
pub fn capture_primary_display_bgra() -> Result<CapturedBgraFrame, String> {
    use windows::Win32::UI::HiDpi::{
        SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    struct DpiGuard(DPI_AWARENESS_CONTEXT);
    impl Drop for DpiGuard {
        fn drop(&mut self) {
            if !self.0 .0.is_null() {
                unsafe {
                    SetThreadDpiAwarenessContext(self.0);
                }
            }
        }
    }
    let _dpi = DpiGuard(unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
    });
    use std::ffi::c_void;
    use std::ptr::null_mut;

    type Bool = i32;
    type Hdc = *mut c_void;
    type Hbitmap = *mut c_void;
    type Hgdiobj = *mut c_void;
    type Hwnd = *mut c_void;

    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;
    const SRCCOPY: u32 = 0x00CC0020;
    const BI_RGB: u32 = 0;
    const DIB_RGB_COLORS: u32 = 0;

    #[repr(C)]
    struct BitmapInfoHeader {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[repr(C)]
    struct RgbQuad {
        rgb_blue: u8,
        rgb_green: u8,
        rgb_red: u8,
        rgb_reserved: u8,
    }

    #[repr(C)]
    struct BitmapInfo {
        bmi_header: BitmapInfoHeader,
        bmi_colors: [RgbQuad; 1],
    }

    #[link(name = "User32")]
    unsafe extern "system" {
        fn GetDC(hwnd: Hwnd) -> Hdc;
        fn GetSystemMetrics(index: i32) -> i32;
        fn ReleaseDC(hwnd: Hwnd, hdc: Hdc) -> i32;
    }

    #[link(name = "Gdi32")]
    unsafe extern "system" {
        fn BitBlt(
            hdc: Hdc,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            hdc_src: Hdc,
            x1: i32,
            y1: i32,
            rop: u32,
        ) -> Bool;
        fn CreateCompatibleBitmap(hdc: Hdc, cx: i32, cy: i32) -> Hbitmap;
        fn CreateCompatibleDC(hdc: Hdc) -> Hdc;
        fn DeleteDC(hdc: Hdc) -> Bool;
        fn DeleteObject(object: Hgdiobj) -> Bool;
        fn GetDIBits(
            hdc: Hdc,
            hbm: Hbitmap,
            start: u32,
            c_lines: u32,
            bits: *mut c_void,
            bmi: *mut BitmapInfo,
            usage: u32,
        ) -> i32;
        fn SelectObject(hdc: Hdc, object: Hgdiobj) -> Hgdiobj;
    }

    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        if width <= 0 || height <= 0 {
            return Err("failed to query primary display size".to_string());
        }

        let screen_dc = GetDC(null_mut());
        if screen_dc.is_null() {
            return Err("failed to acquire primary display device context".to_string());
        }
        let memory_dc = CreateCompatibleDC(screen_dc);
        if memory_dc.is_null() {
            let _ = ReleaseDC(null_mut(), screen_dc);
            return Err("failed to allocate compatible memory device context".to_string());
        }
        let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
        if bitmap.is_null() {
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(null_mut(), screen_dc);
            return Err(
                "failed to allocate compatible bitmap for primary display capture".to_string(),
            );
        }

        let previous = SelectObject(memory_dc, bitmap as Hgdiobj);
        if previous.is_null() {
            let _ = DeleteObject(bitmap as Hgdiobj);
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(null_mut(), screen_dc);
            return Err("failed to select capture bitmap into memory device context".to_string());
        }

        if BitBlt(memory_dc, 0, 0, width, height, screen_dc, 0, 0, SRCCOPY) == 0 {
            let _ = SelectObject(memory_dc, previous);
            let _ = DeleteObject(bitmap as Hgdiobj);
            let _ = DeleteDC(memory_dc);
            let _ = ReleaseDC(null_mut(), screen_dc);
            return Err("failed to copy the primary display into the capture bitmap".to_string());
        }

        let mut bitmap_info = BitmapInfo {
            bmi_header: BitmapInfoHeader {
                bi_size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                bi_width: width,
                bi_height: -height,
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: BI_RGB,
                bi_size_image: (width * height * 4) as u32,
                bi_x_pels_per_meter: 0,
                bi_y_pels_per_meter: 0,
                bi_clr_used: 0,
                bi_clr_important: 0,
            },
            bmi_colors: [RgbQuad {
                rgb_blue: 0,
                rgb_green: 0,
                rgb_red: 0,
                rgb_reserved: 0,
            }],
        };
        let stride = (width as usize) * 4;
        let mut bytes = vec![0u8; stride * (height as usize)];
        let copied_lines = GetDIBits(
            memory_dc,
            bitmap,
            0,
            height as u32,
            bytes.as_mut_ptr() as *mut c_void,
            &mut bitmap_info,
            DIB_RGB_COLORS,
        );

        let _ = SelectObject(memory_dc, previous);
        let _ = DeleteObject(bitmap as Hgdiobj);
        let _ = DeleteDC(memory_dc);
        let _ = ReleaseDC(null_mut(), screen_dc);

        if copied_lines != height {
            return Err(format!(
                "GetDIBits returned {copied_lines} lines, expected {height}"
            ));
        }

        Ok(CapturedBgraFrame {
            width: width as u32,
            height: height as u32,
            stride,
            bytes,
        })
    }
}

#[cfg(not(windows))]
pub fn capture_primary_display_bgra() -> Result<CapturedBgraFrame, String> {
    Err("Primary display capture is only wired for the Windows host crate.".to_string())
}
