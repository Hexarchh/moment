//! 追踪引擎。M2: 会话状态机 + 事件循环 + flush 策略。
//!
//! 会话规则:
//! - 前台应用变化 → 结束旧会话, 开新会话
//! - 标题变化 (同应用) → 会话不切分, 标题在下次 checkpoint 落盘
//! - 空闲开始 → 会话截断到「最后一次输入时刻」(即 now - idle_timeout, 不早于 started),
//!   空闲时段单独记入 idle_periods (恢复输入时一次写入)
//! - 空闲恢复 / 取消暂停 → 由平台层补报的前台窗口事件立刻重启会话
//! - 暂停 → 结束会话且不再计时
//!
//! 异常防护: 会话结束/落盘时刻经单调钟钳制 (capped_now)——系统休眠、时钟前跳时
//! 墙钟差远大于实际醒着的单调时长, 会话长度被压回真实值, 不产生跨休眠的虚长记录;
//! 时钟回拨由 ended_at ≥ started_at 钳制兜底。
//!
//! flush 策略: 会话创建即落盘; 进行中的会话每 `CHECKPOINT_INTERVAL` 滚动更新一次
//! ended_at (崩溃最多丢一个 interval); 退出时统一收尾。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::database::{repo, DbHandle};
use crate::platform::{self, ActiveWindow, PlatformTracker, TrackerEvent};

/// 引擎从事件源取事件的粒度 (也是对 stop 指令的响应上限)
const EVENT_POLL: Duration = Duration::from_secs(2);
/// 进行中会话的落盘间隔
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(10);
/// 平台层故障后的重连间隔
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
/// 单调钳制的宽限秒数 (覆盖事件轮询/调度抖动), 醒着时不会触发
const CLOCK_GRACE_SECS: f64 = 6.0;

/// 进行中的会话 (DB 行 id + 内存态)
struct OpenSession {
    id: i64,
    app_key: String,
    title: Option<String>,
    started_at: chrono::DateTime<Utc>,
    /// 会话开始时的单调钟, 用于休眠/时钟跳变防护
    started_mono: Instant,
    /// 自上次落盘以来标题是否变过
    dirty_title: bool,
}

struct Engine {
    db: DbHandle,
    idle_timeout: Duration,
    paused_flag: Arc<AtomicBool>,
    /// 上一轮观测到的暂停位
    paused: bool,
    idle: bool,
    /// 空闲开始时刻 (墙钟, 即被截断会话的结束时刻), 恢复时落盘成 idle_periods
    idle_since: Option<chrono::DateTime<Utc>>,
    /// 最近一次事件循环醒来的单调时刻, 会话时长的可信上限基准
    last_tick: Instant,
    /// 最近一次前台窗口 (恢复计时时不等窗口切换即可重启会话)
    last_window: Option<ActiveWindow>,
    current: Option<OpenSession>,
}

impl Engine {
    fn new(db: DbHandle, idle_timeout: Duration, paused_flag: Arc<AtomicBool>) -> Self {
        let paused = paused_flag.load(Ordering::Relaxed);
        Self {
            db,
            idle_timeout,
            paused_flag,
            paused,
            idle: false,
            idle_since: None,
            last_tick: Instant::now(),
            last_window: None,
            current: None,
        }
    }

    /// 会话结束时刻的可信上限: 墙钟 now 与「会话开始墙钟 + 实际醒着的单调时长 + 宽限」取小。
    /// 系统休眠或时钟前跳时, 单调时长远小于墙钟差, 会话被钳制——杜绝跨休眠的虚长记录。
    fn capped_now(&self, cur: &OpenSession) -> chrono::DateTime<Utc> {
        let awake = self
            .last_tick
            .saturating_duration_since(cur.started_mono)
            .as_secs_f64();
        let cap = cur.started_at
            + chrono::Duration::milliseconds(((awake + CLOCK_GRACE_SECS) * 1000.0) as i64);
        Utc::now().min(cap)
    }

    /// 同步暂停位, 处理暂停/恢复的会话切换
    fn sync_pause(&mut self) {
        let now = paused_now(&self.paused_flag);
        if now == self.paused {
            return;
        }
        self.paused = now;
        if now {
            tracing::info!("tracking paused");
            self.close_current(Utc::now());
        } else {
            tracing::info!("tracking resumed");
            // 恢复计时: 平台层不感知暂停, 不会补报窗口, 这里自行用 last_window 重启
            if let Some(w) = self.last_window.clone() {
                if !self.idle {
                    self.open_session(&w);
                }
            }
        }
    }

    fn handle_event(&mut self, ev: TrackerEvent) {
        match ev {
            TrackerEvent::ActiveWindow(w) => {
                let title_changed = self
                    .last_window
                    .as_ref()
                    .map(|p| p.app_key == w.app_key && p.title != w.title)
                    .unwrap_or(false);
                self.last_window = Some(w.clone());
                if self.idle || self.paused {
                    return;
                }
                let same_app = matches!(&self.current, Some(cur) if cur.app_key == w.app_key);
                if same_app {
                    if title_changed {
                        if let Some(cur) = &mut self.current {
                            cur.title = w.title.clone();
                            cur.dirty_title = true;
                        }
                    }
                } else {
                    self.close_current(Utc::now());
                    self.open_session(&w);
                }
            }
            TrackerEvent::IdleStarted => {
                self.idle = true;
                // 用户最后一次输入发生在 idle_timeout 之前; 会话截断到该时刻
                let end = Utc::now() - chrono::Duration::from_std(self.idle_timeout).unwrap();
                self.close_current(end);
                self.idle_since = Some(end);
            }
            TrackerEvent::InputResumed => {
                // 平台层紧随其后补报前台窗口, 由 ActiveWindow 分支重启会话
                self.idle = false;
                self.flush_idle(Utc::now());
            }
        }
    }

    fn open_session(&mut self, w: &ActiveWindow) {
        let now = Utc::now();
        let r = self.db.with(|c| {
            let app_id = repo::upsert_application(
                c,
                &w.app_key,
                &w.name,
                w.executable.as_deref(),
                w.path.as_deref(),
                &now,
            )?;
            repo::insert_session(c, app_id, w.title.as_deref(), &now)
        });
        match r {
            Ok(id) => {
                tracing::info!(app = %w.app_key, title = ?w.title, "session opened");
                self.current = Some(OpenSession {
                    id,
                    app_key: w.app_key.clone(),
                    title: w.title.clone(),
                    started_at: now,
                    started_mono: Instant::now(),
                    dirty_title: false,
                });
            }
            Err(e) => tracing::error!("open session 失败: {e}"),
        }
    }

    /// 结束当前会话 (含标题落盘); ended_at 先经单调钳制再 clamp 到 started_at 之后
    fn close_current(&mut self, ended_at: chrono::DateTime<Utc>) {
        let Some(cur) = self.current.take() else {
            return;
        };
        let ended = ended_at.min(self.capped_now(&cur)).max(cur.started_at);
        let title = if cur.dirty_title {
            cur.title.as_deref()
        } else {
            None
        };
        let r = self.db.with(|c| repo::touch_session(c, cur.id, title, &ended));
        if let Err(e) = r {
            tracing::error!("close session 失败: {e}");
        }
    }

    /// 空闲时段落盘 (恢复输入 / 引擎退出时调用), 每段一次写
    fn flush_idle(&mut self, ended_at: chrono::DateTime<Utc>) {
        if let Some(started) = self.idle_since.take() {
            let ended = ended_at.max(started);
            if let Err(e) = self.db.with(|c| repo::insert_idle_period(c, &started, &ended)) {
                tracing::error!("记录空闲时段失败: {e}");
            }
        }
    }

    /// 刷新单调钟基准 (事件循环每轮调用)
    fn tick(&mut self) {
        self.last_tick = Instant::now();
    }

    /// checkpoint: 进行中会话滚动更新 ended_at (经单调钳制)
    fn checkpoint(&mut self) {
        if self.idle || self.paused {
            return;
        }
        let Some(cur) = &self.current else {
            return;
        };
        let (id, title, dirty) = (cur.id, cur.title.clone(), cur.dirty_title);
        let now = self.capped_now(cur);
        let r = self
            .db
            .with(|c| repo::touch_session(c, id, if dirty { title.as_deref() } else { None }, &now));
        match r {
            Ok(()) => {
                if let Some(cur) = &mut self.current {
                    cur.dirty_title = false;
                }
            }
            Err(e) => tracing::error!("checkpoint 失败: {e}"),
        }
    }

    /// 引擎退出前收尾
    fn shutdown(&mut self) {
        self.close_current(Utc::now());
        self.flush_idle(Utc::now());
    }
}

fn paused_now(flag: &AtomicBool) -> bool {
    flag.load(Ordering::Relaxed)
}

/// 引擎线程的管理句柄
pub struct TrackerHandle {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl TrackerHandle {
    /// 通知停止并等待引擎收尾 (阻塞至多 ~EVENT_POLL)
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            if j.join().is_err() {
                tracing::error!("tracker 线程异常退出");
            }
        }
    }
}

/// 启动引擎线程 (含平台事件源)。平台初始化失败时按 RETRY_INTERVAL 重试。
pub fn spawn(db: DbHandle, paused_flag: Arc<AtomicBool>) -> TrackerHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let join = std::thread::Builder::new()
        .name("tracker".to_string())
        .spawn({
            let db = db.clone();
            let stop = stop.clone();
            move || engine_loop(db, paused_flag, stop)
        })
        .expect("spawn tracker thread");
    TrackerHandle { stop, join: Some(join) }
}

fn engine_loop(db: DbHandle, paused_flag: Arc<AtomicBool>, stop: Arc<AtomicBool>) {
    let settings = db
        .with(repo::load_settings)
        .unwrap_or_else(|e| {
            tracing::warn!("读取设置失败, 使用默认: {e}");
            Default::default()
        });
    let idle_timeout = Duration::from_secs(settings.idle_timeout_secs);

    // 清理上次崩溃遗留的未结束会话
    if let Err(e) = db.with(repo::close_orphan_sessions) {
        tracing::warn!("清理遗留会话失败: {e}");
    }

    let mut tracker: Box<dyn PlatformTracker> = loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match platform::create_tracker(idle_timeout) {
            Ok(t) => break t,
            Err(e) => {
                tracing::error!("平台事件源初始化失败, {RETRY_INTERVAL:?} 后重试: {e}");
                std::thread::sleep(RETRY_INTERVAL);
            }
        }
    };

    let mut engine = Engine::new(db, idle_timeout, paused_flag);
    let mut last_checkpoint = Instant::now();

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        engine.sync_pause();

        match tracker.next_event(EVENT_POLL) {
            Ok(Some(ev)) => engine.handle_event(ev),
            Ok(None) => {}
            Err(e) => {
                tracing::error!("平台事件源错误: {e}, 尝试重建");
                loop {
                    if stop.load(Ordering::Relaxed) {
                        engine.shutdown();
                        return;
                    }
                    match platform::create_tracker(idle_timeout) {
                        Ok(t) => {
                            tracker = t;
                            break;
                        }
                        Err(e) => {
                            tracing::error!("重建失败, {RETRY_INTERVAL:?} 后重试: {e}");
                            std::thread::sleep(RETRY_INTERVAL);
                        }
                    }
                }
                engine.tick();
                engine.checkpoint();
                last_checkpoint = Instant::now();
                continue;
            }
        }

        // 单调钟基准: 每轮醒来都刷新, 是会话时长可信上限的依据
        engine.tick();

        if last_checkpoint.elapsed() >= CHECKPOINT_INTERVAL {
            engine.checkpoint();
            last_checkpoint = Instant::now();
        }
    }

    engine.shutdown();
    tracing::info!("tracker engine stopped");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::ActiveWindow;
    use chrono::{DateTime, TimeZone};

    fn test_db() -> DbHandle {
        // 原子序号保证并行测试的临时库路径唯一 (Instant 在时钟粒度内可重复)
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "moment-test-{}-{n}.db",
            std::process::id()
        ));
        let db = crate::database::Db::open(&path).expect("open test db");
        let _ = std::fs::remove_file(&path); // SQLite 句柄仍有效, 退出即清
        db
    }

    fn engine() -> (Engine, Arc<AtomicBool>) {
        let flag = Arc::new(AtomicBool::new(false));
        (Engine::new(test_db(), Duration::from_secs(60), flag.clone()), flag)
    }

    fn window(app: &str, title: &str) -> ActiveWindow {
        ActiveWindow {
            app_key: app.into(),
            name: app.into(),
            title: Some(title.into()),
            executable: None,
            path: None,
        }
    }

    /// (session id, app_key, title, started_at, ended_at)
    type Row = (i64, String, Option<String>, String, Option<String>);

    fn rows(db: &DbHandle) -> Vec<Row> {
        db.with(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT s.id, a.app_key, s.title, s.started_at, s.ended_at
                     FROM sessions s JOIN applications a ON a.id = s.application_id
                     ORDER BY s.id",
                )
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })
                .unwrap();
            Ok(rows.collect::<Result<Vec<_>, _>>().unwrap())
        })
        .unwrap()
    }

    #[test]
    fn app_switch_closes_and_opens() {
        let (mut e, _) = engine();
        e.handle_event(TrackerEvent::ActiveWindow(window("chrome", "A")));
        e.handle_event(TrackerEvent::ActiveWindow(window("kitty", "B")));
        e.shutdown();

        let rs = rows(&e.db);
        assert_eq!(rs.len(), 2);
        assert_eq!(rs[0].1, "chrome");
        assert!(rs[0].4.is_some(), "旧会话已结束");
        assert_eq!(rs[1].1, "kitty");
    }

    #[test]
    fn title_change_keeps_session_and_updates_title() {
        let (mut e, _) = engine();
        e.handle_event(TrackerEvent::ActiveWindow(window("kitty", "old")));
        e.handle_event(TrackerEvent::ActiveWindow(window("kitty", "new")));
        e.shutdown();

        let rs = rows(&e.db);
        assert_eq!(rs.len(), 1, "同应用标题变化不切会话");
        assert_eq!(rs[0].2.as_deref(), Some("new"), "落盘最终标题");
    }

    #[test]
    fn idle_closes_resumed_reopens() {
        let (mut e, _) = engine();
        e.handle_event(TrackerEvent::ActiveWindow(window("chrome", "A")));
        e.handle_event(TrackerEvent::IdleStarted);
        assert!(e.current.is_none(), "空闲即结束会话");
        e.handle_event(TrackerEvent::InputResumed);
        // 平台层补报前台窗口
        e.handle_event(TrackerEvent::ActiveWindow(window("chrome", "A")));
        assert!(e.current.is_some(), "恢复后同窗口也重启会话");
        e.shutdown();

        let rs = rows(&e.db);
        assert_eq!(rs.len(), 2, "空闲前后各一段");
    }

    #[test]
    fn idle_session_clamped_to_last_input() {
        let (mut e, _) = engine();
        e.handle_event(TrackerEvent::ActiveWindow(window("chrome", "A")));
        // 等待使其超过 idle_timeout? 不引入 sleep: 直接构造未来的空闲时刻即可验证 clamp 逻辑
        let started = e.current.as_ref().unwrap().started_at;
        let end = started - chrono::Duration::seconds(30); // 早于 started
        e.close_current(end);
        let rs = rows(&e.db);
        assert_eq!(rs[0].4, Some(rs[0].3.clone()), "ended_at 不早于 started_at");
    }

    #[test]
    fn pause_closes_and_resume_reopens_without_event() {
        let (mut e, flag) = engine();
        e.handle_event(TrackerEvent::ActiveWindow(window("chrome", "A")));
        flag.store(true, Ordering::Relaxed);
        e.sync_pause();
        assert!(e.current.is_none(), "暂停结束会话");
        flag.store(false, Ordering::Relaxed);
        e.sync_pause();
        assert!(e.current.is_some(), "恢复用 last_window 重启, 不等新事件");
        e.shutdown();

        let rs = rows(&e.db);
        assert_eq!(rs.len(), 2);
    }

    #[test]
    fn checkpoint_rolls_ended_at() {
        let (mut e, _) = engine();
        e.handle_event(TrackerEvent::ActiveWindow(window("chrome", "A")));
        e.checkpoint();
        let rs = rows(&e.db);
        assert!(rs[0].4.is_some(), "checkpoint 已写 ended_at");
        assert!(e.current.is_some(), "会话仍在进行");

        // 时间真的前进 (checkpoint 写的是 now, 与 started_at 可比较即可)
        let ended = DateTime::parse_from_rfc3339(&rs[0].4.clone().unwrap())
            .unwrap()
            .with_timezone(&Utc);
        let started = DateTime::parse_from_rfc3339(&rs[0].3).unwrap().with_timezone(&Utc);
        assert!(ended >= started);
        let _ = Utc.timestamp_opt(0, 0).unwrap();
    }

    #[test]
    fn idle_period_recorded_on_resume() {
        let (mut e, _) = engine();
        e.handle_event(TrackerEvent::ActiveWindow(window("chrome", "A")));
        e.handle_event(TrackerEvent::IdleStarted);
        assert!(e.idle_since.is_some());
        e.handle_event(TrackerEvent::InputResumed);
        assert!(e.idle_since.is_none(), "恢复时已落盘");
        e.shutdown();
        let n: i64 = e
            .db
            .with(|c| c.query_row("SELECT COUNT(*) FROM idle_periods", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(n, 1, "恰好一段空闲记录");
    }

    #[test]
    fn idle_period_flushed_on_shutdown_while_idle() {
        let (mut e, _) = engine();
        e.handle_event(TrackerEvent::ActiveWindow(window("chrome", "A")));
        e.handle_event(TrackerEvent::IdleStarted);
        e.shutdown(); // 空闲中退出也要落盘
        let n: i64 = e
            .db
            .with(|c| c.query_row("SELECT COUNT(*) FROM idle_periods", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn clock_forward_jump_clamped_by_monotonic_cap() {
        let (mut e, _) = engine();
        e.handle_event(TrackerEvent::ActiveWindow(window("chrome", "A")));
        // 模拟系统时钟前跳 1 小时: 会话墙钟起点被拨回 1h 前, 但单调时长仍 ≈ 0
        if let Some(cur) = &mut e.current {
            cur.started_at -= chrono::Duration::hours(1);
        }
        e.tick();
        e.shutdown();
        let rs = rows(&e.db);
        let ended = DateTime::parse_from_rfc3339(&rs[0].4.clone().unwrap())
            .unwrap()
            .with_timezone(&Utc);
        let started = DateTime::parse_from_rfc3339(&rs[0].3).unwrap().with_timezone(&Utc);
        let len = (ended - started).num_seconds();
        assert!(len < 60, "时钟前跳被钳制, 会话长度 {len}s 而非 1h");
    }
}
