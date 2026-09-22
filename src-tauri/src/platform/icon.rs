//! 应用图标解析 (Linux): app_id → 查 `.desktop` 文件的 `Icon=` 字段
//! → 解析图标主题路径 → 读文件 → base64。
//! 只在应用首次被前端请求时做文件系统查找, 结果进程内缓存 (每个应用一次)。
//! 图标不落库: 路径/主题可能变化, 缓存足够。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use base64::Engine as _;

#[derive(Debug, Clone, serde::Serialize)]
pub struct IconData {
    /// data URL 的 mime 部分 (image/png | image/svg+xml)
    pub mime: String,
    /// 文件内容 base64
    pub data: String,
}

type Cached = Option<IconData>;

pub fn resolve_app_icon(app_key: &str) -> Option<IconData> {
    static CACHE: OnceLock<Mutex<HashMap<String, Cached>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let mut guard = cache.lock().expect("icon cache poisoned");
    if let Some(hit) = guard.get(app_key) {
        return hit.clone();
    }
    let resolved = find(app_key);
    guard.insert(app_key.to_string(), resolved.clone());
    resolved
}

fn find(app_key: &str) -> Option<IconData> {
    let name = desktop_icon_name(app_key)?;
    let path = resolve_icon_path(&name)?;
    let bytes = std::fs::read(&path).ok()?;
    // 防御: 超大图标不进内存
    if bytes.len() > 512 * 1024 {
        return None;
    }
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("svg") => "image/svg+xml",
        _ => "image/png",
    };
    Some(IconData {
        mime: mime.to_string(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

/// XDG 应用目录 (applications 后缀已拼好)
fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        dirs.push(PathBuf::from(home).join("applications"));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|v| !v.is_empty())
        .map(|v| std::env::split_paths(&v).map(|p| p.join("applications")).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            ["/usr/local/share", "/usr/share"]
                .iter()
                .map(|p| PathBuf::from(p).join("applications"))
                .collect()
        });
    dirs.extend(data_dirs);
    dirs
}

/// 找到 app_key 对应的 .desktop 并读取 Icon= 值
fn desktop_icon_name(app_key: &str) -> Option<String> {
    let dirs = desktop_dirs();
    // 1) 文件名精确匹配 (最常见: app_id 即 desktop 文件主名)
    for d in &dirs {
        let p = d.join(format!("{app_key}.desktop"));
        if p.is_file() {
            if let Some(icon) = read_icon_field(&p) {
                return Some(icon);
            }
        }
    }
    // 2) StartupWMClass 匹配 (app_id 与文件名不一致时)
    for d in &dirs {
        let Ok(entries) = std::fs::read_dir(d) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&p) else {
                continue;
            };
            let wm_class = content
                .lines()
                .find_map(|l| l.strip_prefix("StartupWMClass="))
                .map(str::trim);
            if wm_class == Some(app_key) {
                if let Some(icon) = read_icon_field_from(&content) {
                    return Some(icon);
                }
            }
        }
    }
    None
}

fn read_icon_field(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path).ok().and_then(|c| read_icon_field_from(&c))
}

fn read_icon_field_from(content: &str) -> Option<String> {
    content
        .lines()
        .find_map(|l| l.strip_prefix("Icon="))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Icon= 值 → 实际文件: 绝对路径直取; 图标名在 hicolor 各尺寸/pixmaps 下找
fn resolve_icon_path(name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    if name.starts_with('/') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }

    // 优先小尺寸 (前端只显示 ~22px), svg 兜底
    const SIZES: [&str; 9] = [
        "48x48", "64x64", "32x32", "96x96", "128x128", "256x256", "512x512", "24x24", "16x16",
    ];
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let local = PathBuf::from(home).join(".local/share/icons/hicolor");
        for s in SIZES {
            roots.push(local.join(s).join("apps"));
        }
        roots.push(local.join("scalable").join("apps"));
    }
    for s in SIZES {
        roots.push(PathBuf::from("/usr/share/icons/hicolor").join(s).join("apps"));
    }
    roots.push(PathBuf::from("/usr/share/icons/hicolor/scalable/apps"));
    roots.push(PathBuf::from("/usr/share/pixmaps"));

    for root in roots {
        for ext in ["png", "svg"] {
            let p = root.join(format!("{name}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
        // Icon 值本身可能已带扩展名
        let p = root.join(name);
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if ext == "png" || ext == "svg" {
                    return Some(p);
                }
            }
        }
    }
    None
}
