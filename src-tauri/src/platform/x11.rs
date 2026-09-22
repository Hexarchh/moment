//! X11 / XWayland 实现: 1s 轮询 `_NET_ACTIVE_WINDOW` + MIT-SCREEN-SAVER 空闲。
//! 骨架逻辑 (去重/空闲抑制/恢复补报) 见 polling.rs。

use std::time::Duration;

use x11rb::connection::Connection as _;
use x11rb::protocol::screensaver::ConnectionExt as _;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, GetPropertyReply};
use x11rb::rust_connection::RustConnection;

use super::polling::{PollObserver, PollingTracker};
use super::{ActiveWindow, PlatformError, PlatformResult, PlatformTracker};

x11rb::atom_manager! {
    /// 用到的 X atom 一次性 intern
    pub Atoms:
    /// intern 请求的 cookie 集合
    AtomsCookie {
        WM_CLASS,
        _NET_WM_NAME,
        _NET_WM_PID,
        _NET_ACTIVE_WINDOW,
    }
}

pub(crate) struct X11Observer {
    conn: RustConnection,
    root: x11rb::protocol::xproto::Window,
    atoms: Atoms,
    /// screensaver 扩展不可用时置 false, 空闲检测退化为永久活跃
    screensaver_ok: bool,
}

impl X11Observer {
    pub(crate) fn new() -> PlatformResult<Self> {
        let (conn, screen_num) = x11rb::connect(None)
            .map_err(|e| PlatformError::Message(format!("x11 connect: {e}")))?;
        let root = conn.setup().roots[screen_num].root;
        let atoms = Atoms::new(&conn)
            .map_err(|e| PlatformError::Message(format!("x11 intern atoms: {e}")))?
            .reply()
            .map_err(|e| PlatformError::Message(format!("x11 intern atoms: {e}")))?;
        Ok(Self {
            conn,
            root,
            atoms,
            screensaver_ok: true,
        })
    }
}

impl PollObserver for X11Observer {
    fn name(&self) -> &'static str {
        "x11"
    }

    fn window(&mut self) -> Option<ActiveWindow> {
        let reply = match self.conn.get_property(
            false,
            self.root,
            self.atoms._NET_ACTIVE_WINDOW,
            u32::from(AtomEnum::WINDOW),
            0,
            1,
        ) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("x11: query _NET_ACTIVE_WINDOW failed: {e}");
                return None;
            }
        };
        let window = reply
            .reply()
            .ok()
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .unwrap_or(0);
        if window == 0 {
            // 无 EWMH 前台窗口 (桌面/无窗口), 保持上次观测
            return None;
        }

        let title = self
            .window_string(window, self.atoms._NET_WM_NAME, u32::from(AtomEnum::ANY))
            .or_else(|| {
                self.window_string(window, u32::from(AtomEnum::WM_NAME), u32::from(AtomEnum::ANY))
            });
        let class = self.window_string(window, u32::from(AtomEnum::WM_CLASS), u32::from(AtomEnum::ANY));
        // WM_CLASS = "instance\0class\0", 取 class 段
        let class_name = class
            .as_deref()
            .and_then(|c| c.split('\0').nth(1))
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let pid = self
            .conn
            .get_property(false, window, self.atoms._NET_WM_PID, u32::from(AtomEnum::CARDINAL), 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()));
        let exe_path = pid.and_then(|pid| std::fs::read_link(format!("/proc/{pid}/exe")).ok());
        let exe_basename = exe_path
            .as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        // 归一化应用标识: 优先可执行名, 其次 WM_CLASS
        let app_key = exe_basename
            .clone()
            .or_else(|| class_name.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let name = class_name
            .or_else(|| exe_basename.clone())
            .unwrap_or_else(|| app_key.clone());

        Some(ActiveWindow {
            app_key,
            name,
            title,
            executable: exe_basename,
            path: exe_path.map(|p| p.to_string_lossy().to_string()),
        })
    }

    fn idle_ms(&mut self) -> Option<u64> {
        if !self.screensaver_ok {
            return None;
        }
        match self.conn.screensaver_query_info(self.root) {
            Ok(c) => match c.reply() {
                Ok(info) => Some(u64::from(info.ms_since_user_input)),
                Err(e) => {
                    tracing::warn!("x11: screensaver query failed: {e}");
                    self.screensaver_ok = false;
                    None
                }
            },
            Err(e) => {
                tracing::warn!("x11: screensaver query failed: {e}");
                self.screensaver_ok = false;
                None
            }
        }
    }
}

impl X11Observer {
    fn window_string(&self, window: u32, prop: u32, type_: u32) -> Option<String> {
        let reply: GetPropertyReply = self
            .conn
            .get_property(false, window, prop, type_, 0, 4096)
            .ok()?
            .reply()
            .ok()?;
        let s = String::from_utf8_lossy(&reply.value);
        let s = s.trim_end_matches('\0');
        (!s.is_empty()).then(|| s.to_string())
    }
}

pub(crate) fn create(idle_timeout: Duration) -> PlatformResult<Box<dyn PlatformTracker>> {
    Ok(Box::new(PollingTracker::new(
        X11Observer::new()?,
        idle_timeout,
    )))
}
