//! 建表与迁移。`schema_version` 记录当前结构版本, 后续变更按版本号增量执行。

use crate::error::AppResult;

const SCHEMA_VERSION: i64 = 3;

pub fn migrate(conn: &rusqlite::Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS applications (
            id INTEGER PRIMARY KEY,
            app_key TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            executable TEXT,
            path TEXT,
            first_seen TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY,
            application_id INTEGER NOT NULL REFERENCES applications(id),
            title TEXT,
            started_at TEXT NOT NULL,
            ended_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);
        CREATE INDEX IF NOT EXISTS idx_sessions_app ON sessions(application_id, started_at);
        CREATE TABLE IF NOT EXISTS idle_periods (
            id INTEGER PRIMARY KEY,
            started_at TEXT NOT NULL,
            ended_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_idle_started ON idle_periods(started_at);
        CREATE TABLE IF NOT EXISTS daily_tasks (
            id INTEGER PRIMARY KEY,
            date TEXT NOT NULL,
            title TEXT NOT NULL,
            completed INTEGER NOT NULL DEFAULT 0,
            estimated_minutes INTEGER,
            note TEXT,
            created_at TEXT NOT NULL,
            completed_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_daily_tasks_date ON daily_tasks(date);
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )?;

    // 结构变更按 old_version 分支增量执行;
    // v2 新增 idle_periods (空闲单独记录), v3 新增 daily_tasks (每日计划)
    let current: i64 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if current < SCHEMA_VERSION {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('idle_timeout_secs', '60')
             ON CONFLICT(key) DO NOTHING",
            [],
        )?;
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [SCHEMA_VERSION.to_string()],
        )?;
    }
    Ok(())
}
