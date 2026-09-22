//! X11 / XWayland 实现: 内部 1s 轮询 `_NET_ACTIVE_WINDOW` + screensaver 空闲,
//! 去重后对外表现为事件源 (与 wayland 统一)。
//!
//! 空闲抑制与 wayland 一致: 空闲期间不报窗口事件, 恢复输入时补报一次前台窗口。

use std::time::{Duration, Instant};

use x11rb::connection::Connection as _;
use x11rb::protocol::screensaver::ConnectionExt as _;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, GetPropertyReply};
use x11rb::rust_connection::RustConnection;

use super::{ActiveWindow, PlatformError, PlatformResult, PlatformTracker, TrackerEvent};

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

/// 轮询周期
const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub struct X11Tracker {
    conn: RustConnection,
    root: x11rb::protocol::xproto::Window,
    atoms: Atoms,
    /// 上次观测到的前台窗口 (去重用)
    last: Option<ActiveWindow>,
    idle: bool,
    idle_threshold: Duration,
    /// screensaver 扩展不可用时置 false, 空闲检测退化为永久活跃
    screensaver_ok: bool,
    next_poll: Instant,
    out: std::collections::VecDeque<TrackerEvent>,
}

impl X11Tracker {
    pub fn new(idle_threshold: Duration) -> PlatformResult<Self> {
        let (conn, screen_num) = x11rb::connect(None)
            .map_err(|e| PlatformError::Message(format!("x11 connect: {e}")))?;
        let root = conn.setup().roots[screen_num].root;
        let atoms = Atoms::new(&conn)
            .map_err(|e| PlatformError::Message(format!("x11 intern atoms: {e}")))?
            .reply()
            .map_err(|e| PlatformError::Message(format!("x11 intern atoms: {e}")))?;

        let mut tracker = Self {
            conn,
            root,
            atoms,
            last: None,
            idle: false,
            idle_threshold,
            screensaver_ok: true,
            next_poll: Instant::now(),
            out: Default::default(),
        };
        // 启动即观测一次, 让引擎立刻拿到首个前台窗口
        tracker.poll_once();
        Ok(tracker)
    }

    /// 执行一轮观测, 产出的事件进入 `out`
    fn poll_once(&mut self) {
        self.observe_window();
        self.observe_idle();
        self.next_poll = Instant::now() + POLL_INTERVAL;
    }

    fn observe_window(&mut self) {
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
                return;
            }
        };
        let window = reply
            .reply()
            .ok()
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .unwrap_or(0);
        if window == 0 {
            // 无 EWMH 前台窗口 (桌面/无窗口), 保持上次观测
            return;
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

        let current = ActiveWindow {
            app_key,
            name,
            title,
            executable: exe_basename,
            path: exe_path.map(|p| p.to_string_lossy().to_string()),
        };

        let changed = match &self.last {
            None => true,
            Some(prev) => prev.app_key != current.app_key || prev.title != current.title,
        };
        self.last = Some(current.clone());
        if changed && !self.idle {
            tracing::debug!(app = %current.app_key, title = ?current.title, "x11: active window");
            self.out.push_back(TrackerEvent::ActiveWindow(current));
        }
    }

    fn observe_idle(&mut self) {
        if !self.screensaver_ok {
            return;
        }
        let idle_ms = match self.conn.screensaver_query_info(self.root) {
            Ok(c) => match c.reply() {
                Ok(info) => info.ms_since_user_input,
                Err(e) => {
                    tracing::warn!("x11: screensaver query failed: {e}");
                    self.screensaver_ok = false;
                    return;
                }
            },
            Err(e) => {
                tracing::warn!("x11: screensaver query failed: {e}");
                self.screensaver_ok = false;
                return;
            }
        };

        let idle_now = Duration::from_millis(u64::from(idle_ms)) >= self.idle_threshold;
        match (idle_now, self.idle) {
            (true, false) => {
                self.idle = true;
                self.out.push_back(TrackerEvent::IdleStarted);
                tracing::debug!("x11: idle started");
            }
            (false, true) => {
                self.idle = false;
                self.out.push_back(TrackerEvent::InputResumed);
                if let Some(w) = self.last.clone() {
                    self.out.push_back(TrackerEvent::ActiveWindow(w));
                }
                tracing::debug!("x11: input resumed");
            }
            _ => {}
        }
    }

    fn window_string(
        &self,
        window: u32,
        prop: u32,
        type_: u32,
    ) -> Option<String> {
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

impl PlatformTracker for X11Tracker {
    fn name(&self) -> &'static str {
        "x11"
    }

    fn next_event(&mut self, timeout: Duration) -> PlatformResult<Option<TrackerEvent>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(ev) = self.out.pop_front() {
                return Ok(Some(ev));
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            if now >= self.next_poll {
                self.poll_once();
                continue;
            }
            // 睡到下一个轮询点或 deadline, 取较早者
            let wake = self.next_poll.min(deadline);
            std::thread::sleep(wake.saturating_duration_since(Instant::now()));
        }
    }
}
