//! Wayland 实现: wlr-foreign-toplevel (前台窗口) + ext-idle-notify (空闲)。
//! 纯事件驱动: `next_event` 阻塞在 socket 上, 超时由 poll 控制, 供引擎调度 checkpoint。
//!
//! 空闲抑制: 空闲期间到达的窗口事件只更新内部窗口表, 不对外上报
//! (后台应用的标题变化不应重启计时); 恢复输入时(Resumed)合成上报一次
//! 当前前台窗口, 引擎据此立刻重启会话, 无需等待下一次真实的窗口切换。

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use wayland_client::{
    event_created_child,
    protocol::{wl_display, wl_registry, wl_seat},
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::{self, ExtIdleNotifierV1},
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

use super::{ActiveWindow, PlatformError, PlatformResult, PlatformTracker, TrackerEvent};

const TOPLEVEL_MGR: &str = "zwlr_foreign_toplevel_manager_v1";
const IDLE_NOTIFIER: &str = "ext_idle_notifier_v1";

/// zwlr_foreign_toplevel_handle_v1.state 数组中的 activated 标志位
const STATE_ACTIVATED: u32 = 2;

struct Toplevel {
    handle: ZwlrForeignToplevelHandleV1,
    title: String,
    app_id: String,
    activated: bool,
}

impl Toplevel {
    fn active_window(&self) -> ActiveWindow {
        let key = if self.app_id.is_empty() {
            "unknown".to_string()
        } else {
            self.app_id.clone()
        };
        ActiveWindow {
            app_key: key.clone(),
            name: key,
            title: (!self.title.is_empty()).then(|| self.title.clone()),
            // wayland app_id 即桌面入口名, 无独立可执行信息
            executable: None,
            path: None,
        }
    }
}

/// wayland 连接上的全部状态。dispatch 回调只改这里, 产出的事件进 `out`。
struct WlState {
    manager: Option<ZwlrForeignToplevelManagerV1>,
    notifier: Option<ExtIdleNotifierV1>,
    seats: Vec<wl_seat::WlSeat>,
    notifications: Vec<ExtIdleNotificationV1>,
    toplevels: Vec<Toplevel>,
    idle: bool,
    /// 上次上报的 (app_key, title), 去重用
    last_emitted: Option<(String, Option<String>)>,
    out: VecDeque<TrackerEvent>,
}

impl WlState {
    fn emit_window(&mut self) {
        // 空闲期间不真实上报(标题后台变化等), 恢复时由 Resumed 合成
        if self.idle {
            return;
        }
        if let Some(t) = self.toplevels.iter().find(|t| t.activated) {
            let w = t.active_window();
            let sig = (w.app_key.clone(), w.title.clone());
            if self.last_emitted.as_ref() == Some(&sig) {
                return; // 同一窗口的重复 Done 批次
            }
            self.last_emitted = Some(sig);
            tracing::debug!(app = %w.app_key, title = ?w.title, "wayland: active window");
            self.out.push_back(TrackerEvent::ActiveWindow(w));
        }
    }

    fn emit_resumed(&mut self) {
        self.idle = false;
        self.last_emitted = None; // 强制补报当前前台
        self.out.push_back(TrackerEvent::InputResumed);
        // 空闲期间被抑制的窗口状态在此刻补报, 引擎随即重启会话
        if let Some(t) = self.toplevels.iter().find(|t| t.activated) {
            self.out.push_back(TrackerEvent::ActiveWindow(t.active_window()));
        }
    }
}

pub struct WaylandTracker {
    queue: EventQueue<WlState>,
    state: WlState,
}

impl WaylandTracker {
    pub fn new(idle_timeout: Duration) -> PlatformResult<Self> {
        let conn = Connection::connect_to_env()
            .map_err(|e| PlatformError::Message(format!("wayland connect: {e}")))?;
        let display = conn.display();
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        let _registry = display.get_registry(&qh, ());

        let mut state = WlState {
            manager: None,
            notifier: None,
            seats: Vec::new(),
            notifications: Vec::new(),
            toplevels: Vec::new(),
            idle: false,
            last_emitted: None,
            out: VecDeque::new(),
        };

        // 第一轮 roundtrip: 收集 globals, 绑定 manager / notifier / seat
        queue
            .roundtrip(&mut state)
            .map_err(|e| PlatformError::Message(format!("wayland roundtrip: {e}")))?;
        if state.manager.is_none() {
            return Err(PlatformError::Message(format!(
                "compositor 不支持 {TOPLEVEL_MGR}"
            )));
        }

        // 第二轮 roundtrip: 已绑定的对象补发初始状态 (现存 toplevel 列表)
        queue
            .roundtrip(&mut state)
            .map_err(|e| PlatformError::Message(format!("wayland roundtrip: {e}")))?;

        // 注册空闲通知 (每个 seat 一个), 超时即引擎的空闲阈值
        if let Some(notifier) = state.notifier.clone() {
            let timeout_ms = idle_timeout.as_millis().min(u32::MAX as u128) as u32;
            for seat in &state.seats {
                let n = notifier.get_idle_notification(timeout_ms, seat, &qh, ());
                state.notifications.push(n);
            }
        } else {
            tracing::warn!("wayland: {IDLE_NOTIFIER} 不可用, 空闲检测失效");
        }
        queue
            .roundtrip(&mut state)
            .map_err(|e| PlatformError::Message(format!("wayland roundtrip: {e}")))?;

        Ok(Self { queue, state })
    }

    /// 读 socket 并派发, 直到产出事件或到达 deadline。
    /// 返回 Ok(false) 表示超时。
    fn wait_for_event(&mut self, deadline: Instant) -> PlatformResult<bool> {
        loop {
            if !self.state.out.is_empty() {
                return Ok(true);
            }

            self.queue
                .flush()
                .map_err(|e| PlatformError::Message(format!("wayland flush: {e}")))?;

            match self.queue.prepare_read() {
                Some(guard) => {
                    let fd = guard.connection_fd();
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        return Ok(false);
                    };
                    let mut fds = [PollFd::new(fd, PollFlags::POLLIN)];
                    let timeout = PollTimeout::try_from(remaining).unwrap_or(PollTimeout::MAX);
                    let n = poll(&mut fds, timeout)
                        .map_err(|e| PlatformError::Message(format!("poll: {e}")))?;
                    if n == 0 {
                        return Ok(false); // 超时, 引擎借机 checkpoint
                    }
                    if let Some(rev) = fds[0].revents() {
                        if rev.intersects(PollFlags::POLLERR | PollFlags::POLLHUP) {
                            return Err(PlatformError::Message(
                                "wayland socket 已关闭".to_string(),
                            ));
                        }
                    }
                    guard
                        .read()
                        .map_err(|e| PlatformError::Message(format!("wayland read: {e}")))?;
                }
                None => {
                    // 队列里已有未派发事件
                }
            }

            self.queue
                .dispatch_pending(&mut self.state)
                .map_err(|e| PlatformError::Message(format!("wayland dispatch: {e}")))?;
            if !self.state.out.is_empty() {
                return Ok(true);
            }
            // 读到的这批事件未产生 TrackerEvent (无关窗口更新等), 继续等
        }
    }
}

impl PlatformTracker for WaylandTracker {
    fn name(&self) -> &'static str {
        "wayland"
    }

    fn next_event(&mut self, timeout: Duration) -> PlatformResult<Option<TrackerEvent>> {
        // setup 轮已把初始前台窗口放进 out, 首次调用立即返回
        if let Some(ev) = self.state.out.pop_front() {
            return Ok(Some(ev));
        }
        let deadline = Instant::now() + timeout;
        match self.wait_for_event(deadline)? {
            true => Ok(self.state.out.pop_front()),
            false => Ok(None),
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for WlState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                TOPLEVEL_MGR if version >= 1 => {
                    let v = version.min(3);
                    state.manager = Some(registry.bind(name, v, qh, ()));
                }
                IDLE_NOTIFIER if version >= 1 => {
                    let v = version.min(1);
                    state.notifier = Some(registry.bind(name, v, qh, ()));
                }
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind(name, 1.min(version), qh, ());
                    state.seats.push(seat);
                }
                _ => {}
            },
            // compositor 卸载 global 的场景罕见, 绑定的对象失效由协议错误兜底
            wl_registry::Event::GlobalRemove { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_display::WlDisplay, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &wl_display::WlDisplay,
        event: wl_display::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_display::Event::Error {
            code,
            message,
            object_id,
        } = event
        {
            tracing::error!("wayland protocol error on {object_id:?}: code {code}: {message}");
        }
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for WlState {
    fn event(
        state: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            state.toplevels.push(Toplevel {
                handle: toplevel,
                title: String::new(),
                app_id: String::new(),
                activated: false,
            });
        }
    }

    // toplevel 事件会创建新对象, 必须补上 child 特化, 否则派发时 panic
    event_created_child!(WlState, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for WlState {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(i) = state
            .toplevels
            .iter()
            .position(|t| t.handle.id() == handle.id())
        else {
            return;
        };

        match event {
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                state.toplevels[i].title = title;
            }
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                state.toplevels[i].app_id = app_id;
            }
            zwlr_foreign_toplevel_handle_v1::Event::State { state: raw } => {
                let mut activated = false;
                // wire 格式整数是小端
                for chunk in raw.as_chunks::<4>().0 {
                    let value = u32::from_le_bytes(*chunk);
                    if value == STATE_ACTIVATED {
                        activated = true;
                    }
                }
                state.toplevels[i].activated = activated;
            }
            // 一批属性更新结束, activated 的窗口即当前前台
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                state.emit_window();
            }
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                let t = &state.toplevels[i];
                t.handle.destroy();
                state.toplevels.remove(i);
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotifierV1,
        _: ext_idle_notifier_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotificationV1, ()> for WlState {
    fn event(
        state: &mut Self,
        _: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => {
                state.idle = true;
                state.out.push_back(TrackerEvent::IdleStarted);
                tracing::debug!("wayland: idle started");
            }
            ext_idle_notification_v1::Event::Resumed => {
                state.emit_resumed();
                tracing::debug!("wayland: input resumed");
            }
            _ => {}
        }
    }
}
