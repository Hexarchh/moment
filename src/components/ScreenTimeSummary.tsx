import { deltaPercent } from "../lib/api";

function durationParts(secs: number): { number: string; unit: string }[] {
  const hours = Math.floor(secs / 3600);
  const minutes = Math.floor((secs % 3600) / 60);
  if (hours > 0) return [{ number: String(hours), unit: "小时" }, { number: String(minutes), unit: "分钟" }];
  if (minutes > 0) return [{ number: String(minutes), unit: "分钟" }];
  return [{ number: String(Math.floor(secs)), unit: "秒" }];
}

export function ScreenTimeSummary({
  total,
  compare,
  range,
  loading,
}: {
  total: number;
  compare: number;
  range: "today" | "7d";
  loading: boolean;
}) {
  const delta = deltaPercent(total, compare);
  return (
    <div className="summary-top">
      <div>
        <p className="summary-eyebrow">{range === "today" ? "今日屏幕时间" : "近 7 天屏幕时间"}</p>
        <div className="summary-value" aria-label={loading ? "加载中" : undefined}>
          {loading ? <span className="summary-placeholder">—</span> : durationParts(total).map((part, index) => (
            <span className="duration-part" key={index}><strong>{part.number}</strong><span>{part.unit}</span></span>
          ))}
        </div>
      </div>
      {!loading && delta !== null && (
        <div className="summary-comparison">
          <span>{delta > 0 ? "↑" : delta < 0 ? "↓" : "·"} {Math.abs(delta).toFixed(0)}%</span>
          <small>{range === "today" ? "较昨日" : "较前 7 天"}</small>
        </div>
      )}
    </div>
  );
}
