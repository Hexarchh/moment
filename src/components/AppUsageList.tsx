import type { AppUsage } from "../lib/api";
import { AppUsageRow } from "./AppUsageRow";

export function AppUsageList({ apps, loading, onAppClick }: { apps: AppUsage[]; loading: boolean; onAppClick: () => void }) {
  const max = Math.max(1, ...apps.map((app) => app.total_secs));
  return (
    <section className="most-used-section" aria-labelledby="most-used-title">
      <h2 id="most-used-title">最常使用</h2>
      <div className="usage-list">
        {apps.length ? apps.map((app) => <AppUsageRow key={app.application_id} app={app} max={max} onClick={onAppClick} />) : <p className="usage-empty">{loading ? "正在读取使用记录…" : "从第一次应用切换开始统计。"}</p>}
      </div>
    </section>
  );
}
