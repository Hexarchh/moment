//! Windows 实现: `GetForegroundWindow` 1s 轮询 + `GetLastInputInfo` 空闲检测。
//! 事件式 SetWinEventHook 需要常驻消息循环线程, 1s 轮询在此粒度下开销可忽略,
//! 故采用与 x11 相同的轮询骨架 (去重/空闲抑制/恢复补报见 polling.rs)。
//!
//! 直接声明 Win32 ABI (稳定契约), 不引第三方绑定 crate。
//! 注意: 本文件在 Windows 真机上尚未验证 (见 README 平台支持)。

use std::ffi::c_void;

use super::polling::{PollObserver, PollingTracker};
use super::{ActiveWindow, PlatformResult, PlatformTracker};

type HWND = *mut c_void;
type HANDLE = *mut c_void;

#[repr(C)]
struct LastInputInfo {
    cb_size: u32,
    dw_time: u32,
}

#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> HWND;
    fn GetWindowTextW(hwnd: HWND, buf: *mut u16, max_count: i32) -> i32;
    fn GetWindowThreadProcessId(hwnd: HWND, pid: *mut u32) -> u32;
    fn GetLastInputInfo(info: *mut LastInputInfo) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(desired_access: u32, inherit_handle: i32, pid: u32) -> HANDLE;
    fn QueryFullProcessImageNameW(
        process: HANDLE,
        flags: u32,
        name: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn CloseHandle(obj: HANDLE) -> i32;
    fn GetTickCount() -> u32;
}

const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

pub(crate) struct WindowsObserver {
    /// GetLastInputInfo 失败后置 false, 空闲检测退化为永久活跃
    input_info_ok: bool,
}

impl WindowsObserver {
    pub(crate) fn new() -> Self {
        Self { input_info_ok: true }
    }
}

impl PollObserver for WindowsObserver {
    fn name(&self) -> &'static str {
        "windows"
    }

    fn window(&mut self) -> Option<ActiveWindow> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }

        let title = window_title(hwnd);
        let exe = process_image(hwnd);

        // 归一化: 可执行名去扩展名、小写 (如 Code.exe -> code)
        let basename = exe
            .as_deref()
            .map(std::path::Path::new)
            .and_then(|p| p.file_stem())
            .map(|n| n.to_string_lossy().to_lowercase());
        let app_key = basename.clone().unwrap_or_else(|| "unknown".to_string());

        Some(ActiveWindow {
            app_key: app_key.clone(),
            name: basename.unwrap_or(app_key),
            title,
            executable: exe
                .as_deref()
                .map(std::path::Path::new)
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string()),
            path: exe,
        })
    }

    fn idle_ms(&mut self) -> Option<u64> {
        if !self.input_info_ok {
            return None;
        }
        let mut info = LastInputInfo {
            cb_size: std::mem::size_of::<LastInputInfo>() as u32,
            dw_time: 0,
        };
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            tracing::warn!("windows: GetLastInputInfo 失败, 空闲检测停用");
            self.input_info_ok = false;
            return None;
        }
        // GetTickCount 与 dwTime 同为 32 位毫秒计数, 模 2^32 差值即空闲时长
        let idle = unsafe { GetTickCount() }.wrapping_sub(info.dw_time);
        Some(u64::from(idle))
    }
}

fn window_title(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return None;
    }
    let s = String::from_utf16_lossy(&buf[..n as usize]);
    (!s.is_empty()).then_some(s)
}

/// 前台窗口所属进程的完整可执行路径
fn process_image(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return None;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok =
        unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len) };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

pub(crate) fn create(
    idle_timeout: std::time::Duration,
) -> PlatformResult<Box<dyn PlatformTracker>> {
    Ok(Box::new(PollingTracker::new(
        WindowsObserver::new(),
        idle_timeout,
    )))
}
