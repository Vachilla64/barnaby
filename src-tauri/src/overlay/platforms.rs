//! Window platform scanning and tracking for Barnaby.
//! Detects visible, non-fullscreen OS windows that Barnaby can walk and stand on.

use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use tauri::{AppHandle, Manager};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, ScreenToClient, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, IsZoomed, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

/// A horizontal top-edge platform segment that the pet can stand on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlatformSegment {
    /// Left bound in overlay client physical pixels
    pub x1: f64,
    /// Right bound in overlay client physical pixels
    pub x2: f64,
    /// Top Y position in overlay client physical pixels
    pub y: f64,
    /// Window handle (0 for ground/taskbar floor)
    pub hwnd: isize,
    /// Window title for debugging/logging
    pub title: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Raw window rect in screen coordinates before occlusion calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawWindow {
    pub hwnd: isize,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub title: String,
    pub is_fullscreen: bool,
}

struct EnumContext {
    our_pid: u32,
    screen_x: i32,
    screen_y: i32,
    screen_w: i32,
    screen_h: i32,
    windows: Vec<RawWindow>,
}

/// Computes exposed top-edge platform segments given a list of top-to-bottom Z-ordered windows.
/// Higher windows in Z-order occlude lower windows.
/// Fullscreen windows are ignored and do not produce platforms.
pub fn calculate_platforms(windows: &[RawWindow], min_width: i32) -> Vec<(i32, i32, i32, isize, String)> {
    let mut platforms = Vec::new();

    for i in 0..windows.len() {
        let win = &windows[i];

        // Explicitly ignore fullscreen windows: no standing on them!
        if win.is_fullscreen {
            continue;
        }

        let top_y = win.top;
        let mut intervals: Vec<(i32, i32)> = vec![(win.left, win.right)];

        // Occlude with higher non-fullscreen windows in Z-order (indices 0..i)
        for higher in &windows[0..i] {
            if higher.is_fullscreen {
                continue;
            }
            if higher.top <= top_y && higher.bottom >= top_y {
                let occ_l = higher.left;
                let occ_r = higher.right;

                let mut next = Vec::with_capacity(intervals.len() * 2);
                for (seg_l, seg_r) in intervals {
                    if occ_l <= seg_l && occ_r >= seg_r {
                        // Fully covered by higher window
                        continue;
                    }
                    if occ_l > seg_l && occ_r < seg_r {
                        // Split into two segments
                        next.push((seg_l, occ_l));
                        next.push((occ_r, seg_r));
                        continue;
                    }
                    if occ_l <= seg_l && occ_r > seg_l && occ_r < seg_r {
                        next.push((occ_r, seg_r));
                        continue;
                    }
                    if occ_l > seg_l && occ_l < seg_r && occ_r >= seg_r {
                        next.push((seg_l, occ_l));
                        continue;
                    }
                    next.push((seg_l, seg_r));
                }
                intervals = next;
            }
        }

        for (x1, x2) in intervals {
            if x2 - x1 >= min_width {
                platforms.push((x1, x2, top_y, win.hwnd, win.title.clone()));
            }
        }
    }

    platforms
}

/// Scans visible desktop windows and returns exposed platforms in overlay client pixels.
#[tauri::command]
pub fn scan_window_platforms(app: AppHandle) -> Vec<PlatformSegment> {
    let Some(overlay) = app.get_webview_window("overlay") else {
        return Vec::new();
    };
    let Ok(overlay_hwnd_raw) = overlay.hwnd() else {
        return Vec::new();
    };
    let overlay_hwnd = HWND(overlay_hwnd_raw.0 as *mut _);

    let virtual_bounds = super::monitors::get_virtual_screen_bounds();
    let our_pid = unsafe { GetCurrentProcessId() };

    let mut ctx = EnumContext {
        our_pid,
        screen_x: virtual_bounds.x,
        screen_y: virtual_bounds.y,
        screen_w: virtual_bounds.width,
        screen_h: virtual_bounds.height,
        windows: Vec::with_capacity(32),
    };

    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_proc),
            LPARAM(&mut ctx as *mut EnumContext as isize),
        );
    }

    let raw_platforms = calculate_platforms(&ctx.windows, 32);

    let mut result = Vec::with_capacity(raw_platforms.len());
    for (x1, x2, top_y, hwnd, title) in raw_platforms {
        let mut pt1 = POINT { x: x1, y: top_y };
        let mut pt2 = POINT { x: x2, y: top_y };
        unsafe {
            let _ = ScreenToClient(overlay_hwnd, &mut pt1);
            let _ = ScreenToClient(overlay_hwnd, &mut pt2);
        }
        result.push(PlatformSegment {
            x1: pt1.x as f64,
            x2: pt2.x as f64,
            y: pt1.y as f64,
            hwnd,
            title,
        });
    }

    result
}

/// Fast query for a single window's bounds in overlay client pixels.
/// Used to track a window moving while Barnaby stands on it.
#[tauri::command]
pub fn get_window_bounds(hwnd: isize, app: AppHandle) -> Option<PlatformRect> {
    let overlay = app.get_webview_window("overlay")?;
    let overlay_hwnd = HWND(overlay.hwnd().ok()?.0 as *mut _);
    let target = HWND(hwnd as *mut _);

    unsafe {
        if !IsWindowVisible(target).as_bool() || IsIconic(target).as_bool() {
            return None;
        }

        let mut frame_rect = RECT::default();
        let frame_res = DwmGetWindowAttribute(
            target,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame_rect as *mut _ as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        );

        let r = if frame_res.is_ok() {
            frame_rect
        } else {
            let mut wr = RECT::default();
            GetWindowRect(target, &mut wr).ok()?;
            wr
        };

        let mut pt_tl = POINT { x: r.left, y: r.top };
        let mut pt_br = POINT { x: r.right, y: r.bottom };
        let _ = ScreenToClient(overlay_hwnd, &mut pt_tl);
        let _ = ScreenToClient(overlay_hwnd, &mut pt_br);

        Some(PlatformRect {
            left: pt_tl.x,
            top: pt_tl.y,
            right: pt_br.x,
            bottom: pt_br.y,
        })
    }
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut EnumContext);

    // Skip windows belonging to Barnaby itself
    let mut win_pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut win_pid));
    if win_pid == ctx.our_pid {
        return BOOL(1);
    }

    // Must be visible and not minimized
    if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return BOOL(1);
    }

    // Filter cloaked windows (other virtual desktops, hidden UWP apps)
    let mut cloaked: u32 = 0;
    let res = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut _ as *mut c_void,
        std::mem::size_of::<u32>() as u32,
    );
    if res.is_ok() && cloaked != 0 {
        return BOOL(1);
    }

    // Filter tool windows and non-interactive overlays
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
        return BOOL(1);
    }

    // Filter maximized windows (they touch the top of monitor, no room to stand)
    if IsZoomed(hwnd).as_bool() {
        return BOOL(1);
    }

    // Get accurate DWM extended frame bounds (excludes invisible drop shadow borders)
    let mut frame_rect = RECT::default();
    let frame_res = DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut frame_rect as *mut _ as *mut c_void,
        std::mem::size_of::<RECT>() as u32,
    );

    let (left, top, right, bottom) = if frame_res.is_ok() {
        (
            frame_rect.left,
            frame_rect.top,
            frame_rect.right,
            frame_rect.bottom,
        )
    } else {
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return BOOL(1);
        }
        (r.left, r.top, r.right, r.bottom)
    };

    let width = right - left;
    let height = bottom - top;
    if width <= 48 || height <= 48 {
        return BOOL(1);
    }

    // Check if fullscreen on the virtual desktop or on the current monitor
    let mut is_fullscreen = left <= ctx.screen_x + 12
        && top <= ctx.screen_y + 12
        && right >= ctx.screen_x + ctx.screen_w - 12
        && bottom >= ctx.screen_y + ctx.screen_h - 12;

    if !is_fullscreen {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            let mon_w = mi.rcMonitor.right - mi.rcMonitor.left;
            let mon_h = mi.rcMonitor.bottom - mi.rcMonitor.top;
            if width >= mon_w - 16 && height >= mon_h - 16 {
                is_fullscreen = true;
            } else if top <= mi.rcWork.top + 10 {
                // Window touches top edge of screen: no headroom to stand on top of it
                is_fullscreen = true;
            }
        }
    }

    // Ignore fullscreen windows completely
    if is_fullscreen {
        return BOOL(1);
    }

    let mut title_buf = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut title_buf);
    let title = if len > 0 {
        String::from_utf16_lossy(&title_buf[..len as usize])
    } else {
        String::new()
    };

    ctx.windows.push(RawWindow {
        hwnd: hwnd.0 as isize,
        left,
        top,
        right,
        bottom,
        title,
        is_fullscreen,
    });

    BOOL(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_platforms_basic() {
        let windows = vec![RawWindow {
            hwnd: 100,
            left: 200,
            top: 400,
            right: 800,
            bottom: 900,
            title: "Editor".to_string(),
            is_fullscreen: false,
        }];

        let platforms = calculate_platforms(&windows, 32);
        assert_eq!(platforms.len(), 1);
        assert_eq!(platforms[0], (200, 800, 400, 100, "Editor".to_string()));
    }

    #[test]
    fn test_fullscreen_windows_are_ignored() {
        let windows = vec![
            RawWindow {
                hwnd: 999,
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
                title: "Fullscreen Game".to_string(),
                is_fullscreen: true,
            },
            RawWindow {
                hwnd: 200,
                left: 300,
                top: 500,
                right: 700,
                bottom: 800,
                title: "Terminal".to_string(),
                is_fullscreen: false,
            },
        ];

        let platforms = calculate_platforms(&windows, 32);
        // Only the terminal platform should be present; fullscreen must be ignored!
        assert_eq!(platforms.len(), 1);
        assert_eq!(platforms[0].3, 200);
    }

    #[test]
    fn test_occlusion_splits_platforms() {
        let windows = vec![
            // Window A in front: covers x: 300..500, y: 200..600
            RawWindow {
                hwnd: 1,
                left: 300,
                top: 200,
                right: 500,
                bottom: 600,
                title: "Front Window".to_string(),
                is_fullscreen: false,
            },
            // Window B behind: x: 100..700, y: 300..700 (top edge at y=300 is covered between 300..500)
            RawWindow {
                hwnd: 2,
                left: 100,
                top: 300,
                right: 700,
                bottom: 700,
                title: "Back Window".to_string(),
                is_fullscreen: false,
            },
        ];

        let platforms = calculate_platforms(&windows, 32);
        // Window 1 has 1 platform: 300..500 at y=200
        // Window 2 has 2 platforms: 100..300 and 500..700 at y=300
        assert_eq!(platforms.len(), 3);
        assert_eq!(platforms[0], (300, 500, 200, 1, "Front Window".to_string()));
        assert_eq!(platforms[1], (100, 300, 300, 2, "Back Window".to_string()));
        assert_eq!(platforms[2], (500, 700, 300, 2, "Back Window".to_string()));
    }
}
