import { useEffect, useMemo, useState } from "react";
import { api, formatDuration } from "../lib/api";
import { ACTIVITY_LEVELS, buildActivityModel, type ActivityDay } from "../lib/activity";

/**
 * Moments 活动热力图: 近一年每日强度。
 * - 挂载时取数一次 (无轮询), 网格 useMemo 一次构建
 * - hover 全部走 CSS (缩放/亮度/tooltip), 不进 React 状态 → 365 格零 re-render
 */
export function ActivityGraph() {
  const [days, setDays] = useState<ActivityDay[] | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .activity(365)
      .then((d) => alive && setDays(d))
      .catch(() => alive && setDays([]));
    return () => {
      alive = false;
    };
  }, []);

  const model = useMemo(() => buildActivityModel(days ?? []), [days]);
  const weeks = model.weeks.length;

  return (
    <section className="activity-card" aria-label="过去一年的活动">
      <div className="activity-head">
        <h2>Moments</h2>
        <span>过去一年</span>
      </div>

      <div className="activity-wrap" style={{ "--weeks": weeks } as React.CSSProperties}>
        <div className="activity-weekdays" aria-hidden="true">
          {model.weekdayLabels.map(({ row, label }) => (
            <span key={row} style={{ gridRow: row + 1 }}>
              {label}
            </span>
          ))}
        </div>

        <div className="activity-scroll">
          <div className="activity-months" aria-hidden="true">
            {model.monthLabels.map(({ col, label }) => (
              <span key={col} style={{ gridColumn: col + 1 }}>
                {label}
              </span>
            ))}
          </div>
          <div className="activity-grid">
            {model.weeks.map((week, col) =>
              week.map((cell, row) =>
                cell.date === null ? (
                  <span key={cell.key} className="activity-cell" style={{ gridColumn: col + 1, gridRow: row + 1 }} />
                ) : (
                  <span
                    key={cell.key}
                    className="activity-cell"
                    style={{ gridColumn: col + 1, gridRow: row + 1 }}
                  >
                    <i className="activity-box" data-level={cell.level} />
                    <span className={`activity-tip${col < 2 ? " tip-start" : col > weeks - 3 ? " tip-end" : ""}`}>
                      <b>{formatDateZh(cell.date)}</b>
                      <em>
                        屏幕时间{" "}
                        {cell.screen_time_minutes > 0
                          ? formatDuration(cell.screen_time_minutes * 60)
                          : "无记录"}
                      </em>
                      {cell.completed_tasks > 0 && <em>完成任务 {cell.completed_tasks} 个</em>}
                    </span>
                  </span>
                ),
              ),
            )}
          </div>
        </div>
      </div>

      <div className="activity-legend">
        <span>少</span>
        {Array.from({ length: ACTIVITY_LEVELS }, (_, level) => (
          <i key={level} className="activity-box" data-level={level} />
        ))}
        <span>多</span>
      </div>
    </section>
  );
}

/** 2026-09-24 → 2026年9月24日 */
function formatDateZh(iso: string): string {
  const d = new Date(`${iso}T00:00:00`);
  return `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日`;
}
