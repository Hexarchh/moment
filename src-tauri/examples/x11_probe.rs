//! X11/XWayland 探测: 单次查询 active window + screensaver idle
//!
//! 运行: cargo run --example x11_probe

use x11rb::connection::{Connection as _, RequestConnection};
use x11rb::protocol::screensaver;
use x11rb::protocol::xproto::{intern_atom, AtomEnum};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (conn, screen_num) = match x11rb::connect(None) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("FATAL: cannot connect to X server: {e}");
            std::process::exit(1);
        }
    };
    let root = conn.setup().roots[screen_num].root;

    let a_active = intern_atom(&conn, false, b"_NET_ACTIVE_WINDOW")?.reply()?.atom;
    let a_net_name = intern_atom(&conn, false, b"_NET_WM_NAME")?.reply()?.atom;
    let a_utf8 = intern_atom(&conn, false, b"UTF8_STRING")?.reply()?.atom;
    let a_pid = intern_atom(&conn, false, b"_NET_WM_PID")?.reply()?.atom;

    let reply = x11rb::protocol::xproto::get_property(
        &conn,
        false,
        root,
        a_active,
        u32::from(AtomEnum::WINDOW),
        0,
        1,
    )?
    .reply()?;
    let window = reply.value32().and_then(|mut v| v.next());

    println!("=== x11 / xwayland probe ===");

    match window {
        None => println!("active window: (none — no EWMH-compliant WM on this display)"),
        Some(w) => {
            println!("active window id: {w:#x}");

            let title = get_string(&conn, w, a_net_name, a_utf8).unwrap_or_default();
            let wm_title = get_string(
                &conn,
                w,
                u32::from(AtomEnum::WM_NAME),
                u32::from(AtomEnum::STRING),
            )
            .unwrap_or_default();
            let class = get_string(
                &conn,
                w,
                u32::from(AtomEnum::WM_CLASS),
                u32::from(AtomEnum::STRING),
            )
            .unwrap_or_default();

            println!("_NET_WM_NAME: {title:?}");
            println!("WM_NAME:      {wm_title:?}");
            println!("WM_CLASS:     {class:?}");

            let pid = x11rb::protocol::xproto::get_property(
                &conn,
                false,
                w,
                a_pid,
                u32::from(AtomEnum::CARDINAL),
                0,
                1,
            )?
            .reply()?;
            if let Some(pid) = pid.value32().and_then(|mut v| v.next()) {
                println!("_NET_WM_PID:  {pid}");
                if let Ok(exe) = std::fs::read_link(format!("/proc/{pid}/exe")) {
                    println!("exe:          {exe:?}");
                }
            } else {
                println!("_NET_WM_PID:  (unset)");
            }
        }
    }

    let idle = screensaver::query_info(&conn, root)?.reply()?;
    println!("screensaver idle: {} ms", idle.ms_since_user_input);

    Ok(())
}

fn get_string<Conn: RequestConnection>(
    conn: &Conn,
    window: u32,
    prop: u32,
    ty: u32,
) -> Result<String, x11rb::errors::ReplyError> {
    let r = x11rb::protocol::xproto::get_property(conn, false, window, prop, ty, 0, 4096)?.reply()?;
    Ok(String::from_utf8_lossy(&r.value)
        .trim_end_matches('\0')
        .to_string())
}
