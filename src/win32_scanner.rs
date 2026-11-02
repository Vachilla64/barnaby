use std::ffi::c_void;
use std::mem::size_of;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetCursorPos, GetForegroundWindow,
    GetGUIThreadInfo, GetWindowLongW, GetWindowRect, GetWindowTextW,
    IsIconic, IsWindowVisible, SetWindowPos, GWL_EXSTYLE, GUITHREADINFO,
    SET_WINDOW_POS_FLAGS, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_TOOLWINDOW,
};

/// Represents the geometric bounds and metadata of a detected top-level OS window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedWindow {
    pub hwnd: isize,
    pub title: String,
    pub bounds: WindowBounds,
    pub is_active: bool,
    pub is_taskbar: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WindowBounds {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    #[allow(dead_code)]
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.left && x <= self.right && y >= self.top && y <= self.bottom
    }
}

struct EnumContext<'a> {
    own_hwnd: Option<HWND>,
    foreground_hwnd: HWND,
    windows: &'a mut Vec<ScannedWindow>,
}

/// Win32 Desktop Scanner and Active Manipulation Manager
pub struct WindowScanner {
    own_hwnd: Option<HWND>,
}

impl WindowScanner {
    pub fn new() -> Self {
        Self { own_hwnd: None }
    }

    /// Set our own overlay window HWND so it gets excluded from collision detection and platform extraction.
    pub fn set_own_hwnd(&mut self, hwnd: isize) {
        self.own_hwnd = Some(HWND(hwnd as *mut c_void));
    }

    /// Perform a full scan of all visible, cloaked-filtered top-level windows in top-to-bottom Z-order.
    pub fn scan_windows(&self) -> Vec<ScannedWindow> {
        let mut scanned: Vec<ScannedWindow> = Vec::with_capacity(32);
        let foreground_hwnd = unsafe { GetForegroundWindow() };

        let mut data = EnumContext {
            own_hwnd: self.own_hwnd,
            foreground_hwnd,
            windows: &mut scanned,
        };

        unsafe {
            let _ = EnumWindows(
                Some(Self::enum_windows_proc),
                LPARAM(&mut data as *mut EnumContext as isize),
            );
        }

        // 2. Scan and append the Windows 11 Taskbar ("Shell_TrayWnd") as a static foundational platform
        if let Some(taskbar) = Self::scan_taskbar() {
            scanned.push(taskbar);
        }

        scanned
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam.0 as *mut EnumContext);

        // Filter 1: Ignore our own overlay window
        if let Some(own) = data.own_hwnd {
            if hwnd == own {
                return BOOL(1);
            }
        }

        // Filter 2: Must be visible and not minimized (IsIconic)
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return BOOL(1);
        }

        // Filter 3: Check DWMWA_CLOAKED (filters out suspended UWP apps, virtual desktop leftovers, hidden shell frames)
        let mut cloaked: u32 = 0;
        let res = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut c_void,
            size_of::<u32>() as u32,
        );
        if res.is_ok() && cloaked != 0 {
            return BOOL(1);
        }

        // Filter 4: Check WS_EX_TOOLWINDOW (floating tooltips, tray flyouts, invisible helper windows)
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
            return BOOL(1);
        }

        // Filter 5: Query DWMWA_EXTENDED_FRAME_BOUNDS instead of GetWindowRect
        // to discard the 7-pixel invisible Windows 11 drop shadow margin.
        let mut frame_rect = RECT::default();
        let frame_res = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame_rect as *mut _ as *mut c_void,
            size_of::<RECT>() as u32,
        );

        let bounds = if frame_res.is_ok() {
            WindowBounds {
                left: frame_rect.left,
                top: frame_rect.top,
                right: frame_rect.right,
                bottom: frame_rect.bottom,
            }
        } else {
            // Fallback to standard window rect
            let mut win_rect = RECT::default();
            if GetWindowRect(hwnd, &mut win_rect).is_err() {
                return BOOL(1);
            }
            WindowBounds {
                left: win_rect.left,
                top: win_rect.top,
                right: win_rect.right,
                bottom: win_rect.bottom,
            }
        };

        // Filter 6: Ignore windows with zero or negative dimensions (minimized/offscreen/hidden)
        if bounds.width() <= 16 || bounds.height() <= 16 {
            return BOOL(1);
        }

        // Extract window title
        let mut title_buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title_buf);
        let title = if len > 0 {
            String::from_utf16_lossy(&title_buf[..len as usize])
        } else {
            String::new()
        };

        let is_active = hwnd == data.foreground_hwnd;

        data.windows.push(ScannedWindow {
            hwnd: hwnd.0 as isize,
            title,
            bounds,
            is_active,
            is_taskbar: false,
        });

        BOOL(1)
    }

    /// Queries the Windows Taskbar ("Shell_TrayWnd") bounds.
    pub fn scan_taskbar() -> Option<ScannedWindow> {
        unsafe {
            let taskbar_hwnd = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null());
            if taskbar_hwnd.is_err() {
                return None;
            }
            let hwnd = taskbar_hwnd.unwrap();
            if hwnd.0.is_null() || !IsWindowVisible(hwnd).as_bool() {
                return None;
            }

            let mut frame_rect = RECT::default();
            let res = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut frame_rect as *mut _ as *mut c_void,
                size_of::<RECT>() as u32,
            );

            let bounds = if res.is_ok() {
                WindowBounds {
                    left: frame_rect.left,
                    top: frame_rect.top,
                    right: frame_rect.right,
                    bottom: frame_rect.bottom,
                }
            } else {
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_err() {
                    return None;
                }
                WindowBounds {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                }
            };

            Some(ScannedWindow {
                hwnd: hwnd.0 as isize,
                title: "Windows Taskbar".to_string(),
                bounds,
                is_active: false,
                is_taskbar: true,
            })
        }
    }

    /// Queries global cursor position in screen coordinates.
    pub fn get_cursor_pos() -> Option<(i32, i32)> {
        unsafe {
            let mut pt = POINT::default();
            if GetCursorPos(&mut pt).is_ok() {
                Some((pt.x, pt.y))
            } else {
                None
            }
        }
    }

    /// Track text caret in active foreground window using `GetGUIThreadInfo`.
    /// Returns screen coordinates (x, y) of the text typing cursor if available.
    pub fn track_text_caret() -> Option<(i32, i32)> {
        unsafe {
            let mut info = GUITHREADINFO {
                cbSize: size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };

            // 0 indicates foreground thread
            if GetGUIThreadInfo(0, &mut info).is_ok() {
                if !info.hwndCaret.0.is_null() {
                    let mut pt = POINT {
                        x: info.rcCaret.left,
                        y: info.rcCaret.bottom,
                    };
                    if ClientToScreen(info.hwndCaret, &mut pt).as_bool() {
                        return Some((pt.x, pt.y));
                    }
                }
            }
            None
        }
    }

    /// Returns how long (in milliseconds) the user has been idle (no keyboard/mouse input).
    pub fn check_user_idle_ms() -> u64 {
        unsafe {
            let mut last_input = LASTINPUTINFO {
                cbSize: size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            if GetLastInputInfo(&mut last_input).as_bool() {
                let tick_count = GetTickCount64();
                tick_count.saturating_sub(last_input.dwTime as u64)
            } else {
                0
            }
        }
    }

    /// Physically nudge a target window by (dx, dy) relative pixels using `SetWindowPos`.
    pub fn nudge_window(hwnd: isize, dx: i32, dy: i32) -> bool {
        unsafe {
            let target = HWND(hwnd as *mut c_void);
            let mut rect = RECT::default();
            if GetWindowRect(target, &mut rect).is_err() {
                return false;
            }

            let flags: SET_WINDOW_POS_FLAGS = SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
            SetWindowPos(
                target,
                None,
                rect.left + dx,
                rect.top + dy,
                0,
                0,
                flags,
            ).is_ok()
        }
    }

    /// Shakes a target window left and right with high-frequency micro nudges.
    pub fn shake_window(hwnd: isize) {
        Self::nudge_window(hwnd, 12, 0);
        std::thread::sleep(std::time::Duration::from_millis(30));
        Self::nudge_window(hwnd, -24, 0);
        std::thread::sleep(std::time::Duration::from_millis(30));
        Self::nudge_window(hwnd, 20, 0);
        std::thread::sleep(std::time::Duration::from_millis(30));
        Self::nudge_window(hwnd, -8, 0);
    }

    /// Get current foreground window title
    pub fn get_foreground_window_title() -> String {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return String::new();
            }
            let mut title_buf = [0u16; 256];
            let len = GetWindowTextW(hwnd, &mut title_buf);
            if len > 0 {
                String::from_utf16_lossy(&title_buf[..len as usize])
            } else {
                String::new()
            }
        }
    }
}
