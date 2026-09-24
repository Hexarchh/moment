//! 读写与查询。所有函数只拿 &Connection, 锁由 `Db::with` 管理。

use chrono::{DateTime, SecondsFormat, Utc};

use crate::models::{AppUsage, PlanTask, Settings};

/// 统一时间戳序列化: RFC3339 UTC 毫秒, 字典序可比较
fn ts(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Millis, true)
}

// ---------- settings ----------

pub fn load_settings(conn: &rusqlite::Connection) -> rusqlite::Result<Settings> {
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'idle_timeout_secs'",
            [],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| if e == rusqlite::Error::QueryReturnedNoRows { Ok(None) } else { Err(e) })?;
    Ok(Settings {
        idle_timeout_secs: v.and_then(|s| s.parse().ok()).unwrap_or(60),
    })
}

pub fn save_settings(conn: &rusqlite::Connection, s: &Settings) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('idle_timeout_secs', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [s.idle_timeout_secs.to_string()],
    )?;
    Ok(())
}

// ---------- 写路径 (引擎) ----------

/// 应用首次出现时插入, 否则刷新展示名 (可执行信息以最新观测为准)
pub fn upsert_application(
    conn: &rusqlite::Connection,
    app_key: &str,
    name: &str,
    executable: Option<&str>,
    path: Option<&str>,
    now: &DateTime<Utc>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO applications (app_key, name, executable, path, first_seen)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(app_key) DO UPDATE SET
            name = CASE WHEN excluded.name != '' THEN excluded.name ELSE applications.name END,
            executable = COALESCE(excluded.executable, applications.executable),
            path = COALESCE(excluded.path, applications.path)",
        rusqlite::params![app_key, name, executable, path, ts(now)],
    )?;
    conn.query_row(
        "SELECT id FROM applications WHERE app_key = ?1",
        [app_key],
        |r| r.get(0),
    )
}

pub fn insert_session(
    conn: &rusqlite::Connection,
    application_id: i64,
    title: Option<&str>,
    started_at: &DateTime<Utc>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO sessions (application_id, title, started_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![application_id, title, ts(started_at)],
    )?;
    Ok(conn.last_insert_rowid())
}

/// checkpoint / 结束会话: 滚动更新 ended_at; title 传 Some 时一并落盘
pub fn touch_session(
    conn: &rusqlite::Connection,
    id: i64,
    title: Option<&str>,
    ended_at: &DateTime<Utc>,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET ended_at = ?2, title = COALESCE(?3, title) WHERE id = ?1",
        rusqlite::params![id, ts(ended_at), title],
    )?;
    Ok(())
}

/// 启动时清理上次崩溃遗留的未结束会话 (ended_at 最多落后一个 checkpoint, 按 started_at 截断)
pub fn close_orphan_sessions(conn: &rusqlite::Connection) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE sessions SET ended_at = started_at WHERE ended_at IS NULL",
        [],
    )
}

/// 记录一段空闲 (引擎在输入恢复时写入, 每段一次, 无高频写)
pub fn insert_idle_period(
    conn: &rusqlite::Connection,
    started_at: &DateTime<Utc>,
    ended_at: &DateTime<Utc>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO idle_periods (started_at, ended_at) VALUES (?1, ?2)",
        rusqlite::params![ts(started_at), ts(ended_at)],
    )?;
    Ok(())
}

// ---------- 每日计划 ----------

/// 某天的任务: 未完成在前 (按创建序), 已完成在后 (按完成时间)
pub fn list_plan_tasks(conn: &rusqlite::Connection, date: &str) -> rusqlite::Result<Vec<PlanTask>> {
    let mut stmt = conn.prepare(
        "SELECT id, date, title, completed, estimated_minutes, note, created_at, completed_at
         FROM daily_tasks WHERE date = ?1
         ORDER BY completed, CASE WHEN completed THEN completed_at ELSE created_at END",
    )?;
    let rows = stmt.query_map([date], plan_task_row)?;
    rows.collect()
}

pub fn insert_plan_task(
    conn: &rusqlite::Connection,
    date: &str,
    title: &str,
    estimated_minutes: Option<i64>,
    note: Option<&str>,
) -> rusqlite::Result<PlanTask> {
    conn.execute(
        "INSERT INTO daily_tasks (date, title, estimated_minutes, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            date,
            title,
            estimated_minutes.filter(|&m| m > 0),
            note.filter(|n| !n.is_empty()),
            ts(&Utc::now())
        ],
    )?;
    get_plan_task(conn, conn.last_insert_rowid())
}

/// 勾选切换: 完成写 completed_at, 取消完成清空
pub fn set_plan_task_completed(
    conn: &rusqlite::Connection,
    id: i64,
    completed: bool,
) -> rusqlite::Result<PlanTask> {
    conn.execute(
        "UPDATE daily_tasks SET
            completed = ?2,
            completed_at = CASE WHEN ?2 THEN ?3 ELSE NULL END
         WHERE id = ?1",
        rusqlite::params![id, completed, ts(&Utc::now())],
    )?;
    get_plan_task(conn, id)
}

/// 编辑保存: 文本字段全量写入 (estimated None / note None 即清除)
pub fn save_plan_task(
    conn: &rusqlite::Connection,
    id: i64,
    title: &str,
    estimated_minutes: Option<i64>,
    note: Option<&str>,
) -> rusqlite::Result<PlanTask> {
    conn.execute(
        "UPDATE daily_tasks SET title = ?2, estimated_minutes = ?3, note = ?4 WHERE id = ?1",
        rusqlite::params![id, title, estimated_minutes, note],
    )?;
    get_plan_task(conn, id)
}

pub fn delete_plan_task(conn: &rusqlite::Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM daily_tasks WHERE id = ?1", [id])?;
    Ok(())
}

fn get_plan_task(conn: &rusqlite::Connection, id: i64) -> rusqlite::Result<PlanTask> {
    conn.query_row(
        "SELECT id, date, title, completed, estimated_minutes, note, created_at, completed_at
         FROM daily_tasks WHERE id = ?1",
        [id],
        plan_task_row,
    )
}

fn plan_task_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PlanTask> {
    Ok(PlanTask {
        id: r.get(0)?,
        date: r.get(1)?,
        title: r.get(2)?,
        completed: r.get::<_, i64>(3)? != 0,
        estimated_minutes: r.get(4)?,
        note: r.get(5)?,
        created_at: r.get(6)?,
        completed_at: r.get(7)?,
    })
}

// ---------- 读路径 (统计) ----------

/// [from, to) 区间内的累计使用秒数
pub fn total_between(
    conn: &rusqlite::Connection,
    from: &DateTime<Utc>,
    to: &DateTime<Utc>,
) -> rusqlite::Result<f64> {
    conn.query_row(
        "SELECT COALESCE(SUM(
            (julianday(MIN(ended_at, ?2)) - julianday(MAX(started_at, ?1))) * 86400.0
        ), 0.0)
         FROM sessions
         WHERE ended_at IS NOT NULL AND started_at < ?2 AND ended_at > ?1",
        rusqlite::params![ts(from), ts(to)],
        |r| r.get(0),
    )
}

/// 区间内按应用聚合的时长 (降序)
pub fn app_totals_between(
    conn: &rusqlite::Connection,
    from: &DateTime<Utc>,
    to: &DateTime<Utc>,
    limit: u64,
) -> rusqlite::Result<Vec<AppUsage>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.app_key, a.name,
                SUM((julianday(MIN(s.ended_at, ?2)) - julianday(MAX(s.started_at, ?1))) * 86400.0) AS total
         FROM sessions s JOIN applications a ON a.id = s.application_id
         WHERE s.ended_at IS NOT NULL AND s.started_at < ?2 AND s.ended_at > ?1
         GROUP BY a.id
         ORDER BY total DESC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![ts(from), ts(to), limit], |r| {
        Ok(AppUsage {
            application_id: r.get(0)?,
            app_key: r.get(1)?,
            name: r.get(2)?,
            total_secs: r.get(3)?,
        })
    })?;
    rows.collect()
}

/// 单应用在区间内的累计秒数 (应用趋势)
pub fn app_total_between(
    conn: &rusqlite::Connection,
    application_id: i64,
    from: &DateTime<Utc>,
    to: &DateTime<Utc>,
) -> rusqlite::Result<f64> {
    conn.query_row(
        "SELECT COALESCE(SUM(
            (julianday(MIN(ended_at, ?3)) - julianday(MAX(started_at, ?2))) * 86400.0
        ), 0.0)
         FROM sessions
         WHERE application_id = ?1 AND ended_at IS NOT NULL AND started_at < ?3 AND ended_at > ?2",
        rusqlite::params![application_id, ts(from), ts(to)],
        |r| r.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::database::schema::migrate(&conn).unwrap();
        conn
    }

    fn seed(conn: &rusqlite::Connection) {
        let base = Utc::with_ymd_and_hms(&Utc, 2026, 9, 23, 10, 0, 0).unwrap();
        let chrome = upsert_application(conn, "chrome", "Chrome", None, None, &base).unwrap();
        let kitty = upsert_application(conn, "kitty", "kitty", None, None, &base).unwrap();
        // chrome: 10:00–10:30, 11:00–11:10; kitty: 10:40–10:50
        insert_session(conn, chrome, Some("A"), &base).unwrap();
        insert_session(conn, kitty, Some("B"), &(base + chrono::Duration::minutes(40))).unwrap();
        insert_session(conn, chrome, Some("C"), &(base + chrono::Duration::hours(1))).unwrap();
        touch_session(conn, 1, None, &(base + chrono::Duration::minutes(30))).unwrap();
        touch_session(conn, 2, None, &(base + chrono::Duration::minutes(50))).unwrap();
        touch_session(conn, 3, None, &(base + chrono::Duration::minutes(70))).unwrap();
    }

    #[test]
    fn totals_aggregate_and_clip() {
        let conn = db();
        seed(&conn);
        let base = Utc::with_ymd_and_hms(&Utc, 2026, 9, 23, 10, 0, 0).unwrap();

        // 全天: 30 + 10 + 10 = 50 分钟 (julianday 浮点换算有亚秒级误差)
        let total = total_between(&conn, &base, &(base + chrono::Duration::hours(24))).unwrap();
        assert!((total - 3000.0).abs() < 0.5, "got {total}");

        // 与查询窗相交的会话被裁剪: 10:15–10:45 窗口计入 chrome [10:15,10:30]=15m + kitty [10:40,10:45]=5m
        let clipped = total_between(
            &conn,
            &(base + chrono::Duration::minutes(15)),
            &(base + chrono::Duration::minutes(45)),
        )
        .unwrap();
        assert!((clipped - 1200.0).abs() < 0.5, "got {clipped}");

        // 不相交的窗口为 0
        let none = total_between(
            &conn,
            &(base + chrono::Duration::hours(3)),
            &(base + chrono::Duration::hours(4)),
        )
        .unwrap();
        assert_eq!(none, 0.0);
    }

    #[test]
    fn app_totals_ranked() {
        let conn = db();
        seed(&conn);
        let base = Utc::with_ymd_and_hms(&Utc, 2026, 9, 23, 10, 0, 0).unwrap();
        let rows = app_totals_between(&conn, &base, &(base + chrono::Duration::hours(24)), 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].app_key, "chrome", "chrome 40m > kitty 10m");
        assert!((rows[0].total_secs - 2400.0).abs() < 0.5);
        assert!((rows[1].total_secs - 600.0).abs() < 0.5);
    }

    #[test]
    fn app_total_between_scopes_to_app() {
        let conn = db();
        seed(&conn);
        let base = Utc::with_ymd_and_hms(&Utc, 2026, 9, 23, 10, 0, 0).unwrap();
        let chrome = conn
            .query_row("SELECT id FROM applications WHERE app_key = 'chrome'", [], |r| r.get::<_, i64>(0))
            .unwrap();
        let total = app_total_between(
            &conn,
            chrome,
            &base,
            &(base + chrono::Duration::hours(24)),
        )
        .unwrap();
        assert!((total - 2400.0).abs() < 0.5, "chrome 40m, got {total}");
    }

    #[test]
    fn idle_periods_roundtrip_and_settings() {
        let conn = db();
        let base = Utc::with_ymd_and_hms(&Utc, 2026, 9, 23, 10, 0, 0).unwrap();
        insert_idle_period(&conn, &base, &(base + chrono::Duration::minutes(7))).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM idle_periods", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);

        let s = Settings { idle_timeout_secs: 120 };
        save_settings(&conn, &s).unwrap();
        assert_eq!(load_settings(&conn).unwrap().idle_timeout_secs, 120);
    }

    #[test]
    fn orphans_closed() {
        let conn = db();
        let base = Utc::with_ymd_and_hms(&Utc, 2026, 9, 23, 10, 0, 0).unwrap();
        seed(&conn);
        insert_session(&conn, 1, Some("open"), &(base + chrono::Duration::hours(5))).unwrap();
        let n = close_orphan_sessions(&conn).unwrap();
        assert_eq!(n, 1);
        let still_open: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions WHERE ended_at IS NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(still_open, 0);
    }

    #[test]
    fn plan_task_crud_and_ordering() {
        let conn = db();
        let a = insert_plan_task(&conn, "2026-09-24", "完成 PA2", Some(120), Some("先看第 3 题")).unwrap();
        let b = insert_plan_task(&conn, "2026-09-24", "学习 vLLM", Some(60), None).unwrap();
        let c = insert_plan_task(&conn, "2026-09-25", "提前写的明天任务", None, None).unwrap();
        assert!(!a.completed && a.estimated_minutes == Some(120));
        assert_eq!(a.date, "2026-09-24");

        // 完成写 completed_at, 取消清空
        let a = set_plan_task_completed(&conn, a.id, true).unwrap();
        assert!(a.completed);
        assert!(a.completed_at.is_some());
        let a = set_plan_task_completed(&conn, a.id, false).unwrap();
        assert!(a.completed_at.is_none());

        // 未完成在前 (创建序), 完成在后
        set_plan_task_completed(&conn, b.id, true).unwrap();
        let today: Vec<String> = list_plan_tasks(&conn, "2026-09-24")
            .unwrap()
            .iter()
            .map(|t| t.title.clone())
            .collect();
        assert_eq!(today, vec!["完成 PA2", "学习 vLLM"], "已完成的 b 排到后面");
        assert_eq!(list_plan_tasks(&conn, "2026-09-25").unwrap().len(), 1);
        assert_eq!(list_plan_tasks(&conn, "2026-09-26").unwrap().len(), 0);
        let _ = c;

        // 编辑全量保存: 清除预估/备注
        save_plan_task(&conn, a.id, "完成 PA2 (重写)", None, None).unwrap();
        let a = list_plan_tasks(&conn, "2026-09-24").unwrap().remove(0);
        assert_eq!(a.title, "完成 PA2 (重写)");
        assert_eq!(a.estimated_minutes, None);
        assert_eq!(a.note, None);

        // 删除
        delete_plan_task(&conn, b.id).unwrap();
        assert_eq!(list_plan_tasks(&conn, "2026-09-24").unwrap().len(), 1);
    }
}
