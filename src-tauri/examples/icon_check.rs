//! 图标解析冒烟: 对若干 app_key 打印解析结果
//! 运行: cargo run --example icon_check

fn main() {
    for key in [
        "google-chrome",
        "kitty",
        "QQ",
        "clash-verge",
        "orca",
        "code",
        "org.gnome.Nautilus",
        "does-not-exist",
    ] {
        match moment_lib::platform::resolve_app_icon(key) {
            Some(icon) => println!(
                "{key:>24} -> {} ({} bytes b64, {})",
                icon.mime,
                icon.data.len(),
                if icon.data.is_empty() { "空" } else { "有数据" }
            ),
            None => println!("{key:>24} -> 未找到"),
        }
    }
}
