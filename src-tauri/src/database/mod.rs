//! SQLite 持久层。
//! `Db` 是 `Arc` 共享的连接包装: 引擎线程写 (会话 flush), command 线程读。
//! WAL 模式下单写多读, 读写都走短临界区, 不存在长锁。

pub mod repository;
mod schema;

use std::sync::{Arc, Mutex};

pub use repository as repo;

use crate::error::AppResult;

pub struct Db {
    conn: Mutex<rusqlite::Connection>,
}

pub type DbHandle = Arc<Db>;

impl Db {
    /// 打开 (或创建) 数据库并跑迁移
    pub fn open(path: &std::path::Path) -> AppResult<DbHandle> {
        let conn = rusqlite::Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        schema::migrate(&conn)?;
        Ok(Arc::new(Self {
            conn: Mutex::new(conn),
        }))
    }

    /// 在锁内执行一段数据库操作
    pub fn with<T>(&self, f: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>) -> AppResult<T> {
        let guard = self.conn.lock().expect("db mutex poisoned");
        f(&guard).map_err(Into::into)
    }
}
