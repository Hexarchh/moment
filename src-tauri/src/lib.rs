mod commands;
mod database;
mod error;
mod models;
// pub 供 examples / 集成测试驱动 (tracker_smoke)
pub mod platform;
mod state;
mod tracker;

use state::AppState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

fn init_tracing() {
    let filter = if cfg!(debug_assertions) {
        tracing_subscriber::EnvFilter::try_new("debug")
    } else {
        tracing_subscriber::EnvFilter::try_new("warn")
    };
    match filter {
        Ok(f) => {
            let _ = tracing_subscriber::fmt().with_env_filter(f).try_init();
        }
        Err(_) => {
            let _ = tracing_subscriber::fmt().try_init();
        }
    }
}

pub fn run() {
    init_tracing();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .manage(setup_state())
        .setup(|app| {
            build_tray(app)?;
            tracing::info!("moment started");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 关闭即隐藏到托盘, 后台继续统计
                if window.label() == "main" {
                    if let Err(e) = window.hide() {
                        tracing::warn!("failed to hide main window: {e}");
                    }
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::health,
            commands::overview,
            commands::apps,
            commands::app_trend,
            commands::app_icon,
            commands::get_settings,
            commands::set_settings,
            commands::set_paused,
            commands::get_autostart,
            commands::set_autostart,
            commands::plan_tasks,
            commands::plan_add,
            commands::plan_toggle,
            commands::plan_save,
            commands::plan_delete
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // 托盘进程内运行; Exit 时先停引擎收尾会话再退出
    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            app_handle.state::<AppState>().shutdown_tracker();
        }
    });
}

/// 打开数据库, 注入 AppState 并启动追踪引擎线程
fn setup_state() -> AppState {
    let dir = dirs_data_dir().expect("无法确定应用数据目录");
    std::fs::create_dir_all(&dir).expect("创建应用数据目录失败");
    let db = database::Db::open(&dir.join("moment.db")).expect("打开数据库失败");
    let state = AppState::new(db);
    let handle = tracker::spawn(state.db(), state.paused_flag());
    state.set_tracker(handle);
    state
}

/// Windows: %LOCALAPPDATA%\Moment；Linux: ~/.local/share/moment。
#[cfg(target_os = "windows")]
fn dirs_data_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(|dir| std::path::PathBuf::from(dir).join("Moment"))
}

#[cfg(not(target_os = "windows"))]
fn dirs_data_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))
        .map(|p| p.join("moment"))
}

type SetupResult = Result<(), Box<dyn std::error::Error>>;

fn build_tray(app: &tauri::App) -> SetupResult {
    let open = MenuItem::with_id(app, "open", "打开主界面", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "暂停统计", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 Moment", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &quit])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("no default window icon")?;

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Moment")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "pause" => toggle_pause(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if let Err(e) = win.show().and_then(|_| win.set_focus()) {
            tracing::warn!("failed to show main window: {e}");
        }
    }
}

fn toggle_pause(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let paused = !state.is_paused();
    state.set_paused(paused);
    update_tray_pause_text(app, paused);
    tracing::info!(paused, "tracking paused state changed");
}

/// 暂停状态变化后同步托盘菜单文案 (托盘与设置页共用)
pub(crate) fn update_tray_pause_text(app: &tauri::AppHandle, paused: bool) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let text = if paused { "恢复统计" } else { "暂停统计" };
        let rebuild = || -> tauri::Result<()> {
            let open = MenuItem::with_id(app, "open", "打开主界面", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", text, true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 Moment", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &pause, &quit])?;
            tray.set_menu(Some(menu))
        };
        if let Err(e) = rebuild() {
            tracing::warn!("failed to rebuild tray menu: {e}");
        }
    }
}
