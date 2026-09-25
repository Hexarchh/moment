// 活动热力图计算层。
// 阈值集中在这里, UI 只消费 level; 后续要调阈值/加指标只改本文件。

/** 与后端 ActivityDay 对应 */
export type ActivityDay = {
  date: string; // 本地日期 YYYY-MM-DD
  screen_time_minutes: number;
  completed_tasks: number;
};

/** 强度分级阈值 (分钟): [0,min1) → 0 … ≥ min3 → 4 */
const LEVEL_THRESHOLDS: readonly [number, number, number] = [30, 120, 240];

export const ACTIVITY_LEVELS = LEVEL_THRESHOLDS.length + 1; // 0..4

/** 屏幕分钟数 → 强度等级 0..4 (0 / ≥1 / ≥30 / ≥120 / ≥240 分钟) */
export function calculateActivityLevel(screenTimeMinutes: number): number {
  if (screenTimeMinutes >= LEVEL_THRESHOLDS[2]) return 4;
  if (screenTimeMinutes >= LEVEL_THRESHOLDS[1]) return 3;
  if (screenTimeMinutes >= LEVEL_THRESHOLDS[0]) return 2;
  if (screenTimeMinutes >= 1) return 1;
  return 0;
}

/** 热力图的一格 */
export type ActivityCell = {
  key: string;
  /** 在统计范围内的日期; 范围外/未来的格子为 null */
  date: string | null;
  screen_time_minutes: number;
  completed_tasks: number;
  level: number;
};

export type ActivityModel = {
  /** 每周一列, 周一到周日七行 */
  weeks: ActivityCell[][];
  /** 月份标签: 列号 + 文案 */
  monthLabels: { col: number; label: string }[];
  /** 一/三/五 行标签 */
  weekdayLabels: { row: number; label: string }[];
};

function toISO(d: Date): string {
  const m = `${d.getMonth() + 1}`.padStart(2, "0");
  const day = `${d.getDate()}`.padStart(2, "0");
  return `${d.getFullYear()}-${m}-${day}`;
}

/**
 * 构建 365 天的周网格:
 * - 起点 = 今天 - 364 天, 向前对齐到周一; 首尾不满一周用空格补齐
 * - 未来与范围外的格子 date = null (透明占位)
 * - 月份标签按真实日期落位, 相邻过近时跳过
 */
export function buildActivityModel(
  days: ActivityDay[],
  totalDays = 365,
  today = new Date(),
): ActivityModel {
  const byDate = new Map(days.map((d) => [d.date, d]));

  const start = new Date(today);
  start.setDate(start.getDate() - (totalDays - 1));
  const leading = (start.getDay() + 6) % 7; // 周一=0
  start.setDate(start.getDate() - leading);

  const todayISO = toISO(today);
  const cells: ActivityCell[] = [];
  for (let i = 0; i < leading; i++) {
    cells.push({
      key: `pad-head-${i}`,
      date: null,
      screen_time_minutes: 0,
      completed_tasks: 0,
      level: -1,
    });
  }
  // 从对齐后的周一起逐日推进, 直到今天 (含) —— 区间长度 = totalDays + leading
  const cursor = new Date(start);
  while (toISO(cursor) <= todayISO) {
    const iso = toISO(cursor);
    const day = byDate.get(iso);
    cells.push({
      key: iso,
      date: iso,
      screen_time_minutes: day?.screen_time_minutes ?? 0,
      completed_tasks: day?.completed_tasks ?? 0,
      level: day ? calculateActivityLevel(day.screen_time_minutes) : -1,
    });
    cursor.setDate(cursor.getDate() + 1);
  }
  while (cells.length % 7 !== 0) {
    cells.push({
      key: `pad-tail-${cells.length}`,
      date: null,
      screen_time_minutes: 0,
      completed_tasks: 0,
      level: -1,
    });
  }

  const weeks: ActivityCell[][] = [];
  for (let i = 0; i < cells.length; i += 7) {
    weeks.push(cells.slice(i, i + 7));
  }

  // 月份标签: 某周内出现当月 1~7 号且与上一标签相隔 ≥3 列时落位
  const monthLabels: { col: number; label: string }[] = [];
  let lastCol = -10;
  let lastMonth = -1;
  weeks.forEach((_week, col) => {
    const monday = new Date(start);
    monday.setDate(monday.getDate() + col * 7);
    const month = monday.getMonth();
    if (month !== lastMonth && monday.getDate() <= 7 && col - lastCol >= 3) {
      monthLabels.push({
        col,
        label: `${month + 1}月`,
      });
      lastCol = col;
    }
    lastMonth = month;
  });

  return {
    weeks,
    monthLabels,
    weekdayLabels: [
      { row: 0, label: "一" },
      { row: 2, label: "三" },
      { row: 4, label: "五" },
    ],
  };
}
