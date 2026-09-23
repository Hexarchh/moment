import type { Bar } from "../lib/api";
import { formatDuration } from "../lib/api";
import { categories, type CategoryId } from "../design/categories";

export function WeeklyUsageChart({
  bars,
  todayCategories,
}: {
  bars: Bar[];
  todayCategories?: Partial<Record<CategoryId, number>>;
}) {
  const max = Math.max(3600, ...bars.map((bar) => bar.total_secs));
  const average = bars.length ? bars.reduce((sum, bar) => sum + bar.total_secs, 0) / bars.length : 0;
  const todayTotal = todayCategories ? Object.values(todayCategories).reduce((sum, value) => sum + (value ?? 0), 0) : 0;
  return (
    <div className="chart-block">
      <div className="chart-heading"><h3>每周屏幕时间</h3><span>平均 {formatDuration(average)}</span></div>
      <div className="weekly-plot">
        <div className="chart-grid" aria-hidden="true"><i /><i /><i /></div>
        {average > 0 && <div className="average-line" style={{ bottom: `${(average / max) * 100}%` }} aria-hidden="true" />}
        <div className="weekly-bars">
          {bars.map((bar, index) => {
            const isToday = index === bars.length - 1;
            const height = Math.max(bar.total_secs > 0 ? 4 : 0, (bar.total_secs / max) * 100);
            return (
              <div className="weekly-day" key={`${bar.label}-${index}`} title={`${bar.label} · ${formatDuration(bar.total_secs)}`}>
                <div className={`weekly-column ${isToday ? "today" : ""}`} style={{ height: `${height}%` }}>
                  {isToday && todayCategories && todayTotal > 0 ? categories.map((category) => {
                    const value = todayCategories[category.id] ?? 0;
                    return value > 0 ? <span key={category.id} style={{ height: `${(value / todayTotal) * 100}%`, background: category.color }} /> : null;
                  }) : null}
                </div>
              </div>
            );
          })}
        </div>
      </div>
      <div className="weekly-labels">{bars.map((bar, index) => <span key={`${bar.label}-${index}`} className={index === bars.length - 1 ? "current" : ""}>{bar.label.split(" ")[0]}</span>)}</div>
    </div>
  );
}
