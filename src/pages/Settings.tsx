import { useEffect, useState } from "react";
import { Toggle } from "../components/Toggle";
import { api, type Settings as SettingsData } from "../lib/api";

type Theme = "dark" | "light";
const isWindows = /Windows/i.test(navigator.userAgent);
const dataPath = isWindows
  ? "%LOCALAPPDATA%\\Moment\\moment.db"
  : "~/.local/share/moment/moment.db";

function initialTheme(): Theme {
  try {
    return localStorage.getItem("moment-theme") === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

export function Settings() {
  const [settings, setSettings] = useState<SettingsData | null>(null);
  const [minutes, setMinutes] = useState(1);
  const [paused, setPaused] = useState(false);
  const [saved, setSaved] = useState(false);
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [autostart, setAutostart] = useState<boolean | null>(null);

  useEffect(() => {
    api
      .getSettings()
      .then((s) => {
        setSettings(s);
        setMinutes(Math.max(1, Math.round(s.idle_timeout_secs / 60)));
      })
      .catch(() => {});
    api
      .overview()
      .then((o) => setPaused(o.paused))
      .catch(() => {});
    api
      .getAutostart()
      .then(setAutostart)
      .catch(() => setAutostart(false));
  }, []);

  const applyTheme = (t: Theme) => {
    setTheme(t);
    try {
      localStorage.setItem("moment-theme", t);
    } catch {}
    document.documentElement.classList.toggle("light", t === "light");
  };

  const toggleAutostart = () => {
    if (autostart === null) return;
    const next = !autostart;
    setAutostart(next);
    api.setAutostart(next).catch(() => setAutostart(!next));
  };

  const save = () => {
    api
      .setSettings({ idle_timeout_secs: minutes * 60 })
      .then(() => {
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
      })
      .catch(() => {});
  };

  const togglePause = () => {
    const next = !paused;
    setPaused(next);
    api.setPaused(next).catch(() => setPaused(!next));
  };

  return (
    <div className="page-enter mx-auto max-w-[860px] px-10 py-9">
      <h1 className="text-[22px] font-semibold tracking-tight">设置</h1>

      <div className="mt-8 rounded-card border border-border bg-card p-5">
        <div className="text-[13px] font-medium">外观</div>
        <div className="mt-3 flex items-center justify-between">
          <p className="max-w-[420px] text-[13px] leading-relaxed text-muted">
            界面配色主题，立即生效并记住选择。
          </p>
          <div className="flex overflow-hidden rounded-control border border-border">
            {(
              [
                ["dark", "深色"],
                ["light", "浅色"],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                onClick={() => applyTheme(id)}
                className={`px-3 py-1 text-[12.5px] transition-colors duration-150 ${
                  theme === id ? "bg-surface text-text" : "text-muted hover:text-text"
                }`}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="mt-4 rounded-card border border-border bg-card p-5">
        <div className="text-[13px] font-medium">通用</div>
        <div className="mt-3 flex items-center justify-between">
          <p className="max-w-[420px] text-[13px] leading-relaxed text-muted">
            开机自动启动 Moment 并在后台统计
            {autostart === true && (
              <span className="text-faint">{isWindows ? "（随 Windows 登录启动）" : "（~/.config/autostart）"}</span>
            )}
          </p>
          <Toggle
            on={autostart === true}
            disabled={autostart === null}
            onClick={toggleAutostart}
          />
        </div>
      </div>

      <div className="mt-4 rounded-card border border-border bg-card p-5">
        <div className="text-[13px] font-medium">统计</div>
        <div className="mt-3 flex items-center justify-between">
          <p className="max-w-[420px] text-[13px] leading-relaxed text-muted">
            {paused
              ? "统计已暂停，当前不再记录使用时间。"
              : "Moment 正在后台记录前台应用的使用情况。"}
          </p>
          <button
            onClick={togglePause}
            className={`shrink-0 rounded-control px-3.5 py-1.5 text-[12.5px] font-medium transition-colors duration-150 ${
              paused
                ? "bg-accent text-white hover:bg-accent/85"
                : "border border-border text-muted hover:text-text"
            }`}
          >
            {paused ? "恢复统计" : "暂停统计"}
          </button>
        </div>
      </div>

      <div className="mt-4 rounded-card border border-border bg-card p-5">
        <div className="text-[13px] font-medium">空闲阈值</div>
        <p className="mt-2 max-w-[480px] text-[13px] leading-relaxed text-muted">
          无键盘、鼠标输入超过该时长后停止计时，空闲时段单独记录。
          调整将在下次启动 Moment 时生效。
        </p>
        <div className="mt-4 flex items-center gap-3">
          <input
            type="number"
            min={1}
            max={120}
            value={minutes}
            disabled={settings === null}
            onChange={(e) =>
              setMinutes(Math.min(120, Math.max(1, Number(e.target.value) || 1)))
            }
            className="w-[86px] rounded-control border border-border bg-bg px-3 py-1.5 text-[13px] tabular-nums outline-none focus:border-accent"
          />
          <span className="text-[13px] text-muted">分钟</span>
          <button
            onClick={save}
            disabled={settings === null}
            className="ml-auto rounded-control border border-border px-3.5 py-1.5 text-[12.5px] font-medium text-muted transition-colors duration-150 hover:text-text disabled:opacity-40"
          >
            {saved ? "已保存 ✓" : "保存"}
          </button>
        </div>
      </div>

      <div className="mt-4 rounded-card border border-border bg-card p-5">
        <div className="text-[13px] font-medium">隐私</div>
        <p className="mt-2 max-w-[480px] text-[13px] leading-relaxed text-muted">
          你的数据只保存在这台设备上。所有使用数据存储于本地 SQLite 数据库
          (<code className="text-text">{dataPath}</code>)，
          没有账号、没有云端、没有任何遥测——窗口标题与应用使用情况绝不上传。
        </p>
      </div>
    </div>
  );
}
