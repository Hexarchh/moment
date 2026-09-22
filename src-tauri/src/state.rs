use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::database::DbHandle;
use crate::tracker::TrackerHandle;

/// 全局共享状态: 暂停标志 (与引擎线程共享) + 数据库 + 引擎句柄。
pub struct AppState {
    paused: Arc<AtomicBool>,
    db: DbHandle,
    tracker: Mutex<Option<TrackerHandle>>,
}

impl AppState {
    pub fn new(db: DbHandle) -> Self {
        Self {
            paused: Arc::new(AtomicBool::new(false)),
            db,
            tracker: Mutex::new(None),
        }
    }

    /// 引擎线程持有的暂停标志句柄
    pub fn paused_flag(&self) -> Arc<AtomicBool> {
        self.paused.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn db(&self) -> DbHandle {
        self.db.clone()
    }

    pub fn set_tracker(&self, handle: TrackerHandle) {
        *self.tracker.lock().expect("tracker mutex poisoned") = Some(handle);
    }

    /// 停止引擎并等待收尾 (退出路径调用)
    pub fn shutdown_tracker(&self) {
        if let Some(mut t) = self.tracker.lock().expect("tracker mutex poisoned").take() {
            t.shutdown();
        }
    }
}
