use std::collections::HashSet;
use std::ffi::c_void;
use std::mem::size_of;
use tracing::{error, info};
use windows::core::{w, GUID, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_VALUE_TYPE,
};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABM_GETSTATE, ABM_GETTASKBARPOS, APPBARDATA,
    IVirtualDesktopManager, VirtualDesktopManager,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetCursorPos, GetForegroundWindow,
    GetGUIThreadInfo, GetWindowLongW, GetWindowRect, GetWindowTextW,
    IsIconic, IsWindowVisible, SetWindowPos, GWL_EXSTYLE, GUITHREADINFO,
    HWND_TOPMOST, SET_WINDOW_POS_FLAGS, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, WS_EX_TOOLWINDOW,
};

#[link(name = "winmm")]
extern "system" {
    pub fn timeBeginPeriod(uPeriod: u32) -> u32;
    pub fn timeEndPeriod(uPeriod: u32) -> u32;
}

/// Converts little-endian 16-byte Explorer Registry binary value to `windows::core::GUID`
pub fn guid_from_registry_bytes(b: [u8; 16]) -> GUID {
    GUID::from_values(
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u16::from_le_bytes([b[4], b[5]]),
        u16::from_le_bytes([b[6], b[7]]),
        [b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]],
    )
}

/// Direction the user swiped when changing virtual desktops
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopSwipeDirection {
    /// Swiped to a desktop on the left -> pet enters from the RIGHT edge
    Left,
    /// Swiped to a desktop on the right -> pet enters from the LEFT edge
    Right,
}

#[derive(Debug, Clone, Copy)]
pub struct VirtualDesktopSwitchEvent {
    pub target_guid: GUID,
    pub direction: DesktopSwipeDirection,
}

/// Represents the geometric bounds and metadata of a detected top-level OS window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedWindow {
    pub hwnd: isize,
    pub title: String,
    pub bounds: WindowBounds,
    pub is_active: bool,
    pub is_taskbar: bool,
    pub is_fullscreen: bool,
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
    screen_x: i32,
    screen_y: i32,
    screen_w: i32,
    screen_h: i32,
    windows: &'a mut Vec<ScannedWindow>,
}

/// Win32 Desktop Scanner, Virtual Desktop Tracker, and Active Manipulation Manager
pub struct WindowScanner {
    own_hwnd: Option<HWND>,
    screen_x: i32,
    screen_y: i32,
    screen_w: i32,
    screen_h: i32,
    vdm: Option<IVirtualDesktopManager>,
    last_desktop_id: Option<GUID>,
    last_registry_vd_id: Option<[u8; 16]>,
    last_uncloaked_hwnds: HashSet<isize>,
}

impl WindowScanner {
    pub fn new(screen_x: i32, screen_y: i32, screen_w: i32, screen_h: i32) -> Self {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let vdm: Option<IVirtualDesktopManager> = unsafe {
            CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL).ok()
        };

        let initial_reg_vd = Self::get_current_virtual_desktop_id();

        Self {
            own_hwnd: None,
            screen_x,
            screen_y,
            screen_w,
            screen_h,
            vdm,
            last_desktop_id: None,
            last_registry_vd_id: initial_reg_vd,
            last_uncloaked_hwnds: HashSet::new(),
        }
    }

    pub fn set_screen_bounds(&mut self, x: i32, y: i32, w: i32, h: i32) {
        self.screen_x = x;
        self.screen_y = y;
        self.screen_w = w;
        self.screen_h = h;
    }

    pub fn set_own_hwnd(&mut self, hwnd: isize) {
        self.own_hwnd = Some(HWND(hwnd as *mut c_void));
    }

    /// Fast 60Hz query for a specific window's bounds without full EnumWindows overhead.
    pub fn get_window_bounds_fast(hwnd: isize) -> Option<WindowBounds> {
        unsafe {
            let target = HWND(hwnd as *mut c_void);
            if !IsWindowVisible(target).as_bool() || IsIconic(target).as_bool() {
                return None;
            }

            let mut cloaked: u32 = 0;
            let cloaked_res = DwmGetWindowAttribute(
                target,
                DWMWA_CLOAKED,
                &mut cloaked as *mut _ as *mut c_void,
                size_of::<u32>() as u32,
            );
            if cloaked_res.is_ok() && cloaked != 0 {
                return None;
            }

            let mut frame_rect = RECT::default();
            let res = DwmGetWindowAttribute(
                target,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut frame_rect as *mut _ as *mut c_void,
                size_of::<RECT>() as u32,
            );
            if res.is_ok() {
                Some(WindowBounds {
                    left: frame_rect.left,
                    top: frame_rect.top,
                    right: frame_rect.right,
                    bottom: frame_rect.bottom,
                })
            } else {
                let mut rect = RECT::default();
                if GetWindowRect(target, &mut rect).is_ok() {
                    Some(WindowBounds {
                        left: rect.left,
                        top: rect.top,
                        right: rect.right,
                        bottom: rect.bottom,
                    })
                } else {
                    None
                }
            }
        }
    }

    /// Reads the list of all virtual desktop GUIDs in physical left-to-right order from the Windows 11 registry.
    pub fn get_all_virtual_desktop_ids() -> Vec<[u8; 16]> {
        unsafe {
            let mut hkey = HKEY::default();
            let subkey = w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops");
            if RegOpenKeyExW(HKEY_CURRENT_USER, subkey, 0, KEY_READ, &mut hkey).is_err() {
                return Vec::new();
            }

            let mut buf = [0u8; 512];
            let mut buf_size = 512u32;
            let mut val_type = REG_VALUE_TYPE::default();
            let val_name = w!("VirtualDesktopIDs");

            let res = RegQueryValueExW(
                hkey,
                val_name,
                None,
                Some(&mut val_type),
                Some(buf.as_mut_ptr()),
                Some(&mut buf_size),
            );

            let _ = RegCloseKey(hkey);

            if res.is_ok() && buf_size >= 16 {
                let count = (buf_size / 16) as usize;
                let mut list = Vec::with_capacity(count);
                for chunk in buf[..(count * 16)].chunks_exact(16) {
                    let mut id = [0u8; 16];
                    id.copy_from_slice(chunk);
                    list.push(id);
                }
                list
            } else {
                Vec::new()
            }
        }
    }

    /// Reads the canonical Windows 11 Virtual Desktop ID directly from the Windows Explorer registry.
    pub fn get_current_virtual_desktop_id() -> Option<[u8; 16]> {
        unsafe {
            let mut hkey = HKEY::default();
            let subkey = w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops");
            if RegOpenKeyExW(HKEY_CURRENT_USER, subkey, 0, KEY_READ, &mut hkey).is_err() {
                return None;
            }

            let mut buf = [0u8; 16];
            let mut buf_size = 16u32;
            let mut val_type = REG_VALUE_TYPE::default();
            let val_name = w!("CurrentVirtualDesktop");

            let res = RegQueryValueExW(
                hkey,
                val_name,
                None,
                Some(&mut val_type),
                Some(buf.as_mut_ptr()),
                Some(&mut buf_size),
            );

            let _ = RegCloseKey(hkey);

            if res.is_ok() && buf_size == 16 {
                Some(buf)
            } else {
                None
            }
        }
    }

    /// Moves our transparent overlay window to the target virtual desktop.
    pub fn move_overlay_to_desktop(&self, hwnd: isize, desktop_id: GUID) -> bool {
        if let Some(vdm) = &self.vdm {
            unsafe {
                let target_hwnd = HWND(hwnd as *mut c_void);
                let move_res = vdm.MoveWindowToDesktop(target_hwnd, &desktop_id);
                if let Err(e) = move_res {
                    error!("MoveWindowToDesktop failed: {e}");
                    return false;
                }

                let flags: SET_WINDOW_POS_FLAGS = SWP_NOMOVE
                    | SWP_NOSIZE
                    | SWP_NOACTIVATE
                    | SWP_SHOWWINDOW
                    | SWP_FRAMECHANGED;

                let _ = SetWindowPos(target_hwnd, HWND_TOPMOST, 0, 0, 0, 0, flags);
                info!("Successfully moved overlay HWND 0x{:X} to Desktop GUID {:?}", hwnd, desktop_id);
                return true;
            }
        }
        false
    }

    /// Checks if the user just switched between Windows 11 Virtual Desktops, and computes
    /// the physical swipe direction (`Left` or `Right`) based on the left-to-right desktop order.
    pub fn check_virtual_desktop_switch(&mut self, scanned: &[ScannedWindow]) -> Option<VirtualDesktopSwitchEvent> {
        let mut target_guid = None;
        let mut direction = DesktopSwipeDirection::Right; // Default

        // Layer 1: Canonical Windows 11 Explorer Registry
        if let Some(cur_reg_vd) = Self::get_current_virtual_desktop_id() {
            if let Some(last_reg_vd) = self.last_registry_vd_id {
                if cur_reg_vd != last_reg_vd {
                    let guid = guid_from_registry_bytes(cur_reg_vd);

                    // Compute physical left/right swipe direction based on desktop order
                    let all_desktops = Self::get_all_virtual_desktop_ids();
                    let old_idx = all_desktops.iter().position(|d| *d == last_reg_vd);
                    let new_idx = all_desktops.iter().position(|d| *d == cur_reg_vd);

                    direction = match (old_idx, new_idx) {
                        (Some(o), Some(n)) if n < o => DesktopSwipeDirection::Left,
                        _ => DesktopSwipeDirection::Right,
                    };

                    info!(
                        "Virtual desktop switch: old_idx={:?}, new_idx={:?}, direction={:?}, GUID={:?}",
                        old_idx, new_idx, direction, guid
                    );
                    target_guid = Some(guid);
                }
            }
            self.last_registry_vd_id = Some(cur_reg_vd);
        }

        // Layer 2: IVirtualDesktopManager GUID check on active foreground window fallback
        if target_guid.is_none() {
            if let Some(vdm) = &self.vdm {
                let fg = unsafe { GetForegroundWindow() };
                if !fg.0.is_null() {
                    if let Ok(guid) = unsafe { vdm.GetWindowDesktopId(fg) } {
                        if guid != GUID::zeroed() {
                            if let Some(last_id) = self.last_desktop_id {
                                if last_id != guid {
                                    target_guid = Some(guid);
                                }
                            }
                            self.last_desktop_id = Some(guid);
                        }
                    }
                }
            }
        }

        // Layer 3: Wholesale uncloaked window set transition
        let current_hwnds: HashSet<isize> = scanned
            .iter()
            .filter(|w| !w.is_taskbar && !w.is_fullscreen)
            .map(|w| w.hwnd)
            .collect();

        if target_guid.is_none() && !self.last_uncloaked_hwnds.is_empty() && !current_hwnds.is_empty() {
            let common_count = self.last_uncloaked_hwnds.intersection(&current_hwnds).count();
            if common_count == 0 && (self.last_uncloaked_hwnds.len() >= 2 || current_hwnds.len() >= 2) {
                if let Some(reg_bytes) = Self::get_current_virtual_desktop_id() {
                    target_guid = Some(guid_from_registry_bytes(reg_bytes));
                }
            }
        }

        self.last_uncloaked_hwnds = current_hwnds;

        if let Some(guid) = target_guid {
            self.last_desktop_id = Some(guid);
            Some(VirtualDesktopSwitchEvent {
                target_guid: guid,
                direction,
            })
        } else {
            None
        }
    }

    /// Perform a full scan of all visible, cloaked-filtered top-level windows in top-to-bottom Z-order.
    pub fn scan_windows(&self) -> Vec<ScannedWindow> {
        let mut scanned: Vec<ScannedWindow> = Vec::with_capacity(32);
        let foreground_hwnd = unsafe { GetForegroundWindow() };

        let mut data = EnumContext {
            own_hwnd: self.own_hwnd,
            foreground_hwnd,
            screen_x: self.screen_x,
            screen_y: self.screen_y,
            screen_w: self.screen_w,
            screen_h: self.screen_h,
            windows: &mut scanned,
        };

        unsafe {
            let _ = EnumWindows(
                Some(Self::enum_windows_proc),
                LPARAM(&mut data as *mut EnumContext as isize),
            );
        }

        let foreground_is_fullscreen = scanned.iter().any(|w| w.is_active && w.is_fullscreen);

        if !foreground_is_fullscreen {
            if let Some(taskbar) = Self::scan_taskbar(self.screen_h) {
                scanned.push(taskbar);
            }
        }

        scanned
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam.0 as *mut EnumContext);

        if let Some(own) = data.own_hwnd {
            if hwnd == own {
                return BOOL(1);
            }
        }

        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return BOOL(1);
        }

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

        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
            return BOOL(1);
        }

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

        if bounds.width() <= 16 || bounds.height() <= 16 {
            return BOOL(1);
        }

        let is_fullscreen = bounds.left <= data.screen_x + 8
            && bounds.top <= data.screen_y + 8
            && bounds.right >= data.screen_x + data.screen_w - 8
            && bounds.bottom >= data.screen_y + data.screen_h - 8;

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
            is_fullscreen,
        });

        BOOL(1)
    }

    /// Queries the Windows Taskbar ("Shell_TrayWnd") bounds and checks if it is truly active and not auto-hidden.
    pub fn scan_taskbar(screen_height: i32) -> Option<ScannedWindow> {
        unsafe {
            let taskbar_hwnd = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null());
            if taskbar_hwnd.is_err() {
                return None;
            }
            let hwnd = taskbar_hwnd.unwrap();
            if hwnd.0.is_null() || !IsWindowVisible(hwnd).as_bool() {
                return None;
            }

            let mut cloaked: u32 = 0;
            let cloaked_res = DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut _ as *mut c_void,
                size_of::<u32>() as u32,
            );
            if cloaked_res.is_ok() && cloaked != 0 {
                return None;
            }

            let mut app_bar_data = APPBARDATA {
                cbSize: size_of::<APPBARDATA>() as u32,
                hWnd: hwnd,
                ..Default::default()
            };
            let state = SHAppBarMessage(ABM_GETSTATE, &mut app_bar_data);
            let is_autohide = (state as u32 & 0x01) != 0;

            let mut pos_data = APPBARDATA {
                cbSize: size_of::<APPBARDATA>() as u32,
                hWnd: hwnd,
                ..Default::default()
            };
            let has_pos = SHAppBarMessage(ABM_GETTASKBARPOS, &mut pos_data) != 0;

            let bounds = if has_pos {
                WindowBounds {
                    left: pos_data.rc.left,
                    top: pos_data.rc.top,
                    right: pos_data.rc.right,
                    bottom: pos_data.rc.bottom,
                }
            } else {
                let mut frame_rect = RECT::default();
                let res = DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    &mut frame_rect as *mut _ as *mut c_void,
                    size_of::<RECT>() as u32,
                );
                if res.is_ok() {
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
                }
            };

            if is_autohide || bounds.height() <= 6 || bounds.top >= screen_height - 6 {
                return None;
            }

            Some(ScannedWindow {
                hwnd: hwnd.0 as isize,
                title: "Windows Taskbar".to_string(),
                bounds,
                is_active: false,
                is_taskbar: true,
                is_fullscreen: false,
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
    pub fn track_text_caret() -> Option<(i32, i32)> {
        unsafe {
            let mut info = GUITHREADINFO {
                cbSize: size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };

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

    /// Returns how long (in milliseconds) the user has been idle.
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
