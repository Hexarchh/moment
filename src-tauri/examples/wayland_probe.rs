//! Wayland 协议探测: 实测当前 compositor 对以下协议的支持与行为
//!   - zwlr_foreign_toplevel_manager_v1  (前台窗口检测, 事件驱动)
//!   - ext_idle_notifier_v1              (空闲检测, 事件驱动)
//!
//! 运行: cargo run --example wayland_probe
//! 输出: 全局协议列表 + 实时事件流 (带时间戳)

use std::time::Instant;

use wayland_client::{
    event_created_child,
    protocol::{wl_display, wl_registry, wl_seat},
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::{self, ExtIdleNotifierV1},
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

const TOPLEVEL_MGR: &str = "zwlr_foreign_toplevel_manager_v1";
const IDLE_NOTIFIER: &str = "ext_idle_notifier_v1";
const IDLE_TIMEOUT_MS: u32 = 20_000;

struct Toplevel {
    handle: ZwlrForeignToplevelHandleV1,
    title: String,
    app_id: String,
    activated: bool,
}

struct Probe {
    start: Instant,
    globals: Vec<(u32, String, u32)>,
    seats: Vec<wl_seat::WlSeat>,
    manager: Option<ZwlrForeignToplevelManagerV1>,
    notifier: Option<ExtIdleNotifierV1>,
    toplevels: Vec<Toplevel>,
}

impl Probe {
    fn ts(&self) -> String {
        format!("[{:>7.1}s]", self.start.elapsed().as_secs_f32())
    }
}

fn main() {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("FATAL: cannot connect to wayland display: {e}");
            std::process::exit(1);
        }
    };

    let display = conn.display();
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = display.get_registry(&qh, ());

    let mut probe = Probe {
        start: Instant::now(),
        globals: Vec::new(),
        seats: Vec::new(),
        manager: None,
        notifier: None,
        toplevels: Vec::new(),
    };

    if let Err(e) = queue.roundtrip(&mut probe) {
        eprintln!("FATAL: roundtrip failed: {e}");
        std::process::exit(1);
    }

    println!("=== wayland globals ===");
    for (_, iface, ver) in &probe.globals {
        println!("  {iface} v{ver}");
    }

    let has_toplevel = probe.globals.iter().any(|(_, i, _)| i == TOPLEVEL_MGR);
    let has_idle = probe.globals.iter().any(|(_, i, _)| i == IDLE_NOTIFIER);

    println!();
    println!(
        "{TOPLEVEL_MGR}: {}",
        if has_toplevel { "SUPPORTED" } else { "NOT SUPPORTED" }
    );
    println!(
        "{IDLE_NOTIFIER}:   {}",
        if has_idle { "SUPPORTED" } else { "NOT SUPPORTED" }
    );

    if !has_toplevel && !has_idle {
        eprintln!("neither protocol available, nothing to probe");
        std::process::exit(2);
    }

    // 第二轮 roundtrip: 已绑定的对象会补发初始状态 (现有 toplevel 列表等)
    if let Err(e) = queue.roundtrip(&mut probe) {
        eprintln!("FATAL: second roundtrip failed: {e}");
        std::process::exit(1);
    }

    // 注册空闲通知 (每个 seat 一个)
    if let Some(notifier) = &probe.notifier {
        for seat in &probe.seats {
            notifier.get_idle_notification(IDLE_TIMEOUT_MS, seat, &qh, ());
            println!("{} idle notification registered (timeout {IDLE_TIMEOUT_MS}ms)", probe.ts());
        }
    }

    println!("{} entering event loop (ctrl-c to stop)", probe.ts());
    loop {
        if let Err(e) = queue.blocking_dispatch(&mut probe) {
            eprintln!("FATAL: dispatch failed: {e}");
            std::process::exit(1);
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Probe {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            state.globals.push((name, interface.clone(), version));
            match interface.as_str() {
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
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Probe {
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

impl Dispatch<wl_display::WlDisplay, ()> for Probe {
    fn event(
        _: &mut Self,
        _: &wl_display::WlDisplay,
        event: wl_display::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_display::Event::Error { code, message, object_id } = event {
            eprintln!("wayland protocol error on {object_id:?}: code {code}: {message}");
        }
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for Probe {
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
    event_created_child!(Probe, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for Probe {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let ts = state.ts();
        let Some(i) = state
            .toplevels
            .iter()
            .position(|t| t.handle.id() == handle.id())
        else {
            return;
        };

        match event {
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                println!("{ts} title: {title}");
                state.toplevels[i].title = title;
            }
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                println!("{ts} app_id: {app_id}");
                state.toplevels[i].app_id = app_id;
            }
            zwlr_foreign_toplevel_handle_v1::Event::State { state: raw } => {
                let mut activated = false;
                // wire 格式整数是小端
                for chunk in raw.as_chunks::<4>().0 {
                    let value = u32::from_le_bytes(*chunk);
                    match value {
                        0 => println!("{ts}   state: maximized"),
                        1 => println!("{ts}   state: minimized"),
                        2 => activated = true,
                        v => println!("{ts}   state: unknown({v})"),
                    }
                }
                state.toplevels[i].activated = activated;
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                let t = &state.toplevels[i];
                if t.activated {
                    println!("{ts} ACTIVE => app_id='{}' title='{}'", t.app_id, t.title);
                }
            }
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                let t = &state.toplevels[i];
                t.handle.destroy();
                let app_id = t.app_id.clone();
                state.toplevels.remove(i);
                println!("{ts} closed: app_id='{app_id}'");
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for Probe {
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

impl Dispatch<ExtIdleNotificationV1, ()> for Probe {
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
                println!(
                    "{} IDLE started (no input for {}ms)",
                    state.ts(),
                    IDLE_TIMEOUT_MS
                )
            }
            ext_idle_notification_v1::Event::Resumed => {
                println!("{} IDLE resumed (input detected)", state.ts())
            }
            _ => {}
        }
    }
}
