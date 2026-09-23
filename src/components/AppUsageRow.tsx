import { AppIcon } from "./AppIcon";
import { formatDuration, type AppUsage } from "../lib/api";
import { categoryColor } from "../design/categories";

export function AppUsageRow({ app, max, onClick }: { app: AppUsage; max: number; onClick: () => void }) {
  return (
    <button type="button" className="usage-row" onClick={onClick}>
      <AppIcon appKey={app.app_key} name={app.name} size={40} />
      <div className="usage-row-content">
        <div className="usage-row-top"><span className="usage-app-name">{app.name}</span><span className="usage-duration">{formatDuration(app.total_secs)}</span></div>
        <div className="usage-progress"><span style={{ width: `${Math.max(2, (app.total_secs / max) * 100)}%`, background: categoryColor(app) }} /></div>
      </div>
      <span className="usage-chevron" aria-hidden="true">›</span>
    </button>
  );
}
