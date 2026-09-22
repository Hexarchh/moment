//! OS 抽象层。
//! M1 实现 wayland（wlr-foreign-toplevel + ext-idle-notify，事件驱动）
//! 与 x11 fallback（x11rb，内部 1s 轮询去重）。两种实现对上层统一呈现为事件源。

#[cfg(target_os = "linux")]
pub mod icon;
#[cfg(target_os = "linux")]
pub mod wayland;
#[cfg(target_os = "linux")]
pub mod x11;

#[cfg(target_os = "linux")]
pub use icon::{resolve_app_icon, IconData};

use std::time::Duration;

/// 一次前台窗口观测结果
#[derive(Debug, Clone)]
pub struct ActiveWindow {
    /// 归一化应用标识：wayland app_id / x11 可执行名
    pub app_key: String,
    /// 展示名
    pub name: String,
    /// 窗口标题
    pub title: Option<String>,
    /// 可执行文件名（如 `code`）
    pub executable: Option<String>,
    /// 可执行文件完整路径（如可得）
    pub path: Option<String>,
}

/// 平台层产出的事件。状态机（tracker）只消费事件，不感知 OS。
#[derive(Debug)]
pub enum TrackerEvent {
    /// 前台窗口变化（或仅标题变化，由 engine 决定是否切换 session）
    ActiveWindow(ActiveWindow),
    /// 输入空闲超过阈值
    IdleStarted,
    /// 输入恢复
    InputResumed,
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("platform: {0}")]
    Message(String),
}

pub type PlatformResult<T> = Result<T, PlatformError>;

/// 阻塞式事件源抽象。
/// Wayland 实现阻塞在 socket 上；X11 实现内部以 1s 间隔轮询并去重。
/// `timeout` 用于 engine 的 checkpoint 调度，两种实现都必须遵守。
pub trait PlatformTracker: Send {
    /// 平台名，用于日志
    fn name(&self) -> &'static str;

    /// 阻塞直到事件或超时。`Ok(None)` 表示超时（无事件）。
    fn next_event(&mut self, timeout: Duration) -> PlatformResult<Option<TrackerEvent>>;
}

/// 按环境选择平台实现：wayland 优先（原生事件驱动），失败回落 x11。
/// 两者都不可用时报错（无图形会话）。
pub fn create_tracker(idle_timeout: Duration) -> PlatformResult<Box<dyn PlatformTracker>> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            match wayland::WaylandTracker::new(idle_timeout) {
                Ok(t) => {
                    tracing::info!(platform = t.name(), "platform tracker ready");
                    return Ok(Box::new(t));
                }
                Err(e) => {
                    tracing::warn!("wayland tracker 初始化失败, 回落 x11: {e}");
                }
            }
        }
        if std::env::var_os("DISPLAY").is_some() {
            let t = x11::X11Tracker::new(idle_timeout)?;
            tracing::info!(platform = t.name(), "platform tracker ready");
            return Ok(Box::new(t));
        }
    }
    Err(PlatformError::Message(
        "未检测到图形会话 (WAYLAND_DISPLAY / DISPLAY 均未设置)".to_string(),
    ))
}
