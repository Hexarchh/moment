//! 轮询式平台实现的公共骨架 (x11 / windows / macos 共用)。
//! 子类只提供两个原语: 「查前台窗口」与「查空闲毫秒」;
//! 去重、空闲抑制、恢复补报、超时语义全部由本骨架统一实现。

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::{ActiveWindow, PlatformTracker, TrackerEvent};

/// 轮询周期
pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// 平台原语
pub(crate) trait PollObserver: Send {
    fn name(&self) -> &'static str;
    /// 当前前台窗口; None 表示本次观测失败/无窗口
    fn window(&mut self) -> Option<ActiveWindow>;
    /// 距上次输入的毫秒; None 表示空闲信息不可用 (视为永不空闲)
    fn idle_ms(&mut self) -> Option<u64>;
}

pub(crate) struct PollingTracker<O: PollObserver> {
    observer: O,
    threshold_ms: u64,
    idle: bool,
    last: Option<ActiveWindow>,
    next_poll: Instant,
    out: VecDeque<TrackerEvent>,
}

impl<O: PollObserver> PollingTracker<O> {
    pub(crate) fn new(observer: O, idle_threshold: Duration) -> Self {
        Self {
            observer,
            threshold_ms: idle_threshold.as_millis().min(u64::MAX as u128) as u64,
            idle: false,
            last: None,
            next_poll: Instant::now(),
            out: VecDeque::new(),
        }
    }

    /// 执行一轮观测, 产出的事件进入 `out`
    fn poll_once(&mut self) {
        // 窗口: (app_key, title) 变化才上报; 空闲期间只记忆不上报
        if let Some(w) = self.observer.window() {
            let changed = match &self.last {
                None => true,
                Some(p) => p.app_key != w.app_key || p.title != w.title,
            };
            self.last = Some(w.clone());
            if changed && !self.idle {
                tracing::debug!(app = %w.app_key, title = ?w.title, "active window");
                self.out.push_back(TrackerEvent::ActiveWindow(w));
            }
        }

        // 空闲: 状态翻转才上报; 恢复时补报当前前台, 引擎随即重启会话
        if let Some(ms) = self.observer.idle_ms() {
            let now_idle = ms >= self.threshold_ms;
            match (now_idle, self.idle) {
                (true, false) => {
                    self.idle = true;
                    self.out.push_back(TrackerEvent::IdleStarted);
                }
                (false, true) => {
                    self.idle = false;
                    self.out.push_back(TrackerEvent::InputResumed);
                    if let Some(w) = self.last.clone() {
                        self.out.push_back(TrackerEvent::ActiveWindow(w));
                    }
                }
                _ => {}
            }
        }

        self.next_poll = Instant::now() + POLL_INTERVAL;
    }
}

impl<O: PollObserver> PlatformTracker for PollingTracker<O> {
    fn name(&self) -> &'static str {
        self.observer.name()
    }

    fn next_event(&mut self, timeout: Duration) -> super::PlatformResult<Option<TrackerEvent>> {
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
