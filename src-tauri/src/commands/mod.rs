//! Tauri command 层：薄壳，仅做参数校验与转发，不含业务逻辑。
//! 统计口径: 「某天的使用时长」= 会话区间与该天 [本地0点, 次0点) 的交集。

use chrono::{DateTime, Datelike, Local, TimeZone, Utc};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::database::repo;
use crate::error::{AppError, AppResult};
use crate::models::{AppUsage, Settings};
use crate::state::AppState;

#[tauri::command]
pub fn health() -> &'static str {
    "ok"
}

/// 趋势柱状图的一根柱子
#[derive(Serialize, Clone)]
pub struct Bar {
    /// 展示用标签 (今日=小时 "9"; 近7天=周几 "一"; 近30天=日期 "9/23")
    pub label: String,
    pub total_secs: f64,
}

#[derive(Serialize)]
pub struct Overview {
    pub range: String,
    pub total_secs: f64,
    /// 上一可比周期 (今天→昨天; 近7天→前7天; 近30天→前30天)
    pub compare_secs: f64,
    pub bars: Vec<Bar>,
    /// 今日页同时展示近七天趋势；其他范围复用主图数据
    pub weekly_bars: Vec<Bar>,
    pub top_apps: Vec<AppUsage>,
    pub category_apps: Vec<AppUsage>,
    pub paused: bool,
}

/// 本地某天 0 点对应的 UTC 时刻 (days_ago=0 为今天)
fn local_midnight(days_ago: i64) -> DateTime<Utc> {
    let day = Local::now().date_naive() - chrono::Duration::days(days_ago);
    let midnight = day.and_hms_opt(0, 0, 0).expect("valid midnight");
    Local
        .from_local_datetime(&midnight)
        .single()
        .expect("unambiguous local midnight")
        .with_timezone(&Utc)
}

fn day_bounds(days_ago: i64) -> (DateTime<Utc>, DateTime<Utc>) {
    (local_midnight(days_ago), local_midnight(days_ago - 1))
}

const WEEKDAY_ZH: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];

fn day_label(days_ago: i64, with_weekday: bool) -> String {
    let local = local_midnight(days_ago).with_timezone(&Local);
    let date = format!("{}/{}", local.month(), local.day());
    if with_weekday {
        format!("周{} {date}", WEEKDAY_ZH[local.weekday().num_days_from_monday() as usize])
    } else {
        date
    }
}

/// 首页: 按范围 (today/7d/30d) 返回总时长、对比值、柱状趋势与 top 应用
#[tauri::command]
pub fn overview(state: State<'_, AppState>, range: Option<String>) -> AppResult<Overview> {
    let db = state.db();
    let range = match range.as_deref() {
        Some("7d") => "7d",
        Some("30d") => "30d",
        _ => "today",
    };

    let (bounds, prev_bounds, bars) = match range {
        "7d" | "30d" => {
            let days: i64 = if range == "7d" { 7 } else { 30 };
            let mut bars = Vec::with_capacity(days as usize);
            for i in (0..days).rev() {
                let (s, e) = day_bounds(i);
                let total = db.with(|c| repo::total_between(c, &s, &e))?;
                bars.push(Bar { label: day_label(i, days == 7), total_secs: total });
            }
            (
                (local_midnight(days - 1), local_midnight(-1)), // 含今天
                (local_midnight(2 * days - 1), local_midnight(days - 1)),
                bars,
            )
        }
        _ => {
            let midnight = local_midnight(0);
            let mut bars = Vec::with_capacity(24);
            for h in 0..24 {
                let s = midnight + chrono::Duration::hours(h);
                let e = s + chrono::Duration::hours(1);
                let total = db.with(|c| repo::total_between(c, &s, &e))?;
                bars.push(Bar { label: h.to_string(), total_secs: total });
            }
            (
                day_bounds(0),
                day_bounds(1),
                bars,
            )
        }
    };

    let (start, end) = bounds;
    let (p_start, p_end) = prev_bounds;
    let total_secs = db.with(|c| repo::total_between(c, &start, &end))?;
    let compare_secs = db.with(|c| repo::total_between(c, &p_start, &p_end))?;
    let category_apps = db.with(|c| repo::app_totals_between(c, &start, &end, 500))?;
    let top_apps = category_apps.iter().take(5).cloned().collect();
    let weekly_bars = if range == "today" {
        let mut days = Vec::with_capacity(7);
        for i in (0..7).rev() {
            let (s, e) = day_bounds(i);
            days.push(Bar {
                label: day_label(i, true),
                total_secs: db.with(|c| repo::total_between(c, &s, &e))?,
            });
        }
        days
    } else {
        bars.clone()
    };

    Ok(Overview {
        range: range.to_string(),
        total_secs,
        compare_secs,
        bars,
        weekly_bars,
        top_apps,
        category_apps,
        paused: state.is_paused(),
    })
}

/// 应用排行: 近 N 天 (默认 7)
#[tauri::command]
pub fn apps(state: State<'_, AppState>, days: Option<i64>) -> AppResult<Vec<AppUsage>> {
    let days = days.filter(|d| *d > 0).unwrap_or(7).min(365);
    let start = local_midnight(days - 1);
    let end = local_midnight(-1);
    state.db().with(|c| repo::app_totals_between(c, &start, &end, 50))
}

/// 单应用近 N 天逐日趋势 (默认 30)
#[tauri::command]
pub fn app_trend(state: State<'_, AppState>, app_id: i64, days: Option<i64>) -> AppResult<Vec<Bar>> {
    let days = days.filter(|d| *d > 1).unwrap_or(30).min(90);
    let mut bars = Vec::with_capacity(days as usize);
    for i in (0..days).rev() {
        let (s, e) = day_bounds(i);
        let total = state.db().with(|c| repo::app_total_between(c, app_id, &s, &e))?;
        bars.push(Bar { label: day_label(i, false), total_secs: total });
    }
    Ok(bars)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    state.db().with(repo::load_settings)
}

/// 保存设置。空闲阈值在下次启动时对平台事件源生效。
#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> AppResult<()> {
    state.db().with(|c| repo::save_settings(c, &settings))
}

/// 应用图标 (base64 data-url 内容), 找不到返回 null 由前端回退字母头像
#[tauri::command]
pub fn app_icon(app_key: String) -> Option<crate::platform::IconData> {
    crate::platform::resolve_app_icon(&app_key)
}

/// 设置页/托盘共用的暂停开关; 同步托盘菜单文案
#[tauri::command]
pub fn set_paused(app: AppHandle, state: State<'_, AppState>, paused: bool) -> AppResult<()> {
    state.set_paused(paused);
    crate::update_tray_pause_text(&app, paused);
    Ok(())
}

/// 开机自启状态 (Linux: ~/.config/autostart 下的 desktop 项)
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> AppResult<bool> {
    use tauri_plugin_autostart::ManagerExt as _;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| AppError::Message(format!("读取自启状态失败: {e}")))
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> AppResult<()> {
    use tauri_plugin_autostart::ManagerExt as _;
    let manager = app.autolaunch();
    let r = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    r.map_err(|e| AppError::Message(format!("设置开机自启失败: {e}")))
}
