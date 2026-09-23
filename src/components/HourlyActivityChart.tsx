import type { Bar } from "../lib/api";
import { formatDuration } from "../lib/api";

export function HourlyActivityChart({ bars }: { bars: Bar[] }) {
  const max = Math.max(3600, ...bars.map((bar) => bar.total_secs));
  return (
    <div className="chart-block hourly-block">
      <div className="chart-heading"><h3>每小时活动</h3></div>
      <div className="hourly-plot">
        <div className="chart-grid" aria-hidden="true"><i /><i /><i /></div>
        <div className="hourly-bars">
          {bars.map((bar, index) => <div className="hour-slot" key={index} title={`${String(index).padStart(2, "0")}:00 · ${formatDuration(bar.total_secs)}`}><div className="hour-bar" style={{ height: `${(bar.total_secs / max) * 100}%` }} /></div>)}
        </div>
      </div>
      <div className="hourly-labels"><span>0时</span><span>6时</span><span>12时</span><span>18时</span><span>24时</span></div>
    </div>
  );
}
