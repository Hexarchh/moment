import { invoke } from "@tauri-apps/api/core";
import type { ActivityDay } from "./activity";

export type { ActivityDay } from "./activity";

// 与后端 command 层的返回结构一一对应 (snake_case)
export type AppUsage = {
  application_id: number;
  app_key: string;
  name: string;
  total_secs: number;
};

export type Bar = {
  label: string;
  total_secs: number;
};

export type Overview = {
  range: string;
  total_secs: number;
  compare_secs: number;
  bars: Bar[];
  weekly_bars: Bar[];
  top_apps: AppUsage[];
  category_apps: AppUsage[];
  paused: boolean;
};

export type Settings = {
  idle_timeout_secs: number;
};

export type PlanTask = {
  id: number;
  date: string; // 本地日期 YYYY-MM-DD
  title: string;
  completed: boolean;
  estimated_minutes: number | null;
  note: string | null;
  created_at: string;
  completed_at: string | null;
};

export type IconData = {
  mime: string;
  data: string;
};

export type Range = "today" | "7d" | "30d";

export const api = {
  overview: (range?: Range) => invoke<Overview>("overview", { range }),
  apps: (days?: number) => invoke<AppUsage[]>("apps", { days }),
  appTrend: (appId: number, days?: number) => invoke<Bar[]>("app_trend", { appId, days }),
  appIcon: (appKey: string) => invoke<IconData | null>("app_icon", { appKey }),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
  setPaused: (paused: boolean) => invoke<void>("set_paused", { paused }),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  planTasks: (date?: string) => invoke<PlanTask[]>("plan_tasks", { date }),
  planAdd: (date: string, title: string, estimatedMinutes: number | null, note: string | null) =>
    invoke<PlanTask>("plan_add", { date, title, estimatedMinutes, note }),
  planToggle: (id: number, completed: boolean) =>
    invoke<PlanTask>("plan_toggle", { id, completed }),
  planSave: (id: number, title: string, estimatedMinutes: number | null, note: string | null) =>
    invoke<PlanTask>("plan_save", { id, title, estimatedMinutes, note }),
  planDelete: (id: number) => invoke<void>("plan_delete", { id }),
  activity: (days?: number) => invoke<ActivityDay[]>("activity", { days }),
};

/** 本地日期 → YYYY-MM-DD (默认今天; offsetDays 正数为过去) */
export function localDateString(offsetDays = 0, base = new Date()): string {
  const d = new Date(base);
  d.setDate(d.getDate() - offsetDays);
  const m = `${d.getMonth() + 1}`.padStart(2, "0");
  const day = `${d.getDate()}`.padStart(2, "0");
  return `${d.getFullYear()}-${m}-${day}`;
}

/** 分钟 → "2 小时" / "1 小时 30 分" / "30 分钟" */
export function formatMinutes(mins: number): string {
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  if (h > 0) return m > 0 ? `${h} 小时 ${m} 分` : `${h} 小时`;
  return `${m} 分钟`;
}

/** 秒 → "3小时24分" / "18分" / "42秒" */
export function formatDuration(secs: number): string {
  if (secs <= 0 || Number.isNaN(secs)) return "0秒";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 0) return m > 0 ? `${h}小时${m}分` : `${h}小时`;
  if (m > 0) return `${m}分`;
  return `${s}秒`;
}

/** 与上一周期对比: 返回 null 表示无可比数据 */
export function deltaPercent(current: number, compare: number): number | null {
  if (compare <= 0) return null;
  return ((current - compare) / compare) * 100;
}

// 应用色: 对 app_key 稳定哈希到一组低饱和色板
const PALETTE = [
  "#0a84ff", "#64d2ff", "#bf5af2", "#ff9f0a", "#8e8e93",
];

export function appColor(appKey: string): string {
  let h = 0;
  for (let i = 0; i < appKey.length; i++) {
    h = (h * 31 + appKey.charCodeAt(i)) >>> 0;
  }
  return PALETTE[h % PALETTE.length];
}

// 图标 data URL 进程内缓存 (每个应用只查一次)
const iconCache = new Map<string, string | null>();

export async function appIconUrl(appKey: string): Promise<string | null> {
  if (iconCache.has(appKey)) {
    return iconCache.get(appKey) ?? null;
  }
  let url: string | null = null;
  try {
    const icon = await api.appIcon(appKey);
    if (icon && icon.data) {
      url = `data:${icon.mime};base64,${icon.data}`;
    }
  } catch {
    url = null;
  }
  iconCache.set(appKey, url);
  return url;
}
