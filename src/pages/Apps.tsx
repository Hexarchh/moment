import { useEffect, useState } from "react";
import { AppIcon } from "../components/AppIcon";
import { api, appColor, formatDuration, type AppUsage, type Bar } from "../lib/api";

const RANGES = [
  { days: 7, label: "近 7 天" },
  { days: 30, label: "近 30 天" },
] as const;

export function Apps() {
  const [days, setDays] = useState<number>(7);
  const [apps, setApps] = useState<AppUsage[] | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);

  useEffect(() => {
    let alive = true;
    setApps(null);
    setExpanded(null);
    api
      .apps(days)
      .then((a) => alive && setApps(a))
      .catch(() => alive && setApps([]));
    return () => {
      alive = false;
    };
  }, [days]);

  const max = Math.max(...(apps ?? []).map((a) => a.total_secs), 1);
  const total = (apps ?? []).reduce((acc, a) => acc + a.total_secs, 0);

  return (
    <div className="page-enter mx-auto max-w-[860px] px-10 py-9">
      <header className="flex items-center justify-between">
        <h1 className="text-[22px] font-semibold tracking-tight">应用</h1>
        <div className="flex overflow-hidden rounded-control border border-border">
          {RANGES.map((r) => (
            <button
              key={r.days}
              onClick={() => setDays(r.days)}
              className={`px-3 py-1 text-[12.5px] transition-colors duration-150 ${
                days === r.days ? "bg-card text-text" : "text-muted hover:text-text"
              }`}
            >
              {r.label}
            </button>
          ))}
        </div>
      </header>

      <section className="mt-8">
        <div className="text-[44px] font-semibold leading-none tracking-tight tabular-nums">
          {apps ? formatDuration(total) : "—"}
        </div>
        <div className="mt-2 text-[13px] text-muted">
          近 {days} 天 · {apps?.length ?? 0} 个应用
        </div>
      </section>

      {apps === null ? (
        <div className="mt-8 rounded-card border border-border bg-card px-5 py-8 text-center text-[13px] text-faint">
          加载中…
        </div>
      ) : apps.length === 0 ? (
        <div className="mt-8 rounded-card border border-border bg-card px-5 py-8 text-center text-[13px] text-faint">
          暂无使用记录。
        </div>
      ) : (
        <div className="mt-8 overflow-hidden rounded-card border border-border bg-card">
          {apps.map((a, i) => (
            <AppRow
              key={a.application_id}
              app={a}
              rank={i + 1}
              max={max}
              pct={total > 0 ? Math.round((a.total_secs / total) * 100) : 0}
              expanded={expanded === a.application_id}
              onToggle={() =>
                setExpanded(expanded === a.application_id ? null : a.application_id)
              }
            />
          ))}
        </div>
      )}
    </div>
  );
}

function AppRow({
  app,
  rank,
  max,
  pct,
  expanded,
  onToggle,
}: {
  app: AppUsage;
  rank: number;
  max: number;
  pct: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  return (
    <div className="border-b border-border-soft last:border-b-0">
      <button
        onClick={onToggle}
        className="flex w-full items-center gap-4 px-5 py-3.5 text-left transition-colors duration-150 hover:bg-surface"
      >
        <span className="w-6 shrink-0 text-right text-[12.5px] tabular-nums text-faint">
          {rank}
        </span>
        <AppIcon appKey={app.app_key} name={app.name} />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline justify-between">
            <span className="truncate text-[13.5px] font-medium">{app.name}</span>
            <span className="ml-3 shrink-0 text-[13px] tabular-nums text-muted">
              {formatDuration(app.total_secs)}
              {pct > 0 && (
                <span className="ml-2 text-[12px] text-faint">{pct}%</span>
              )}
            </span>
          </div>
          <div className="mt-2 h-[3px] overflow-hidden rounded-full bg-border">
            <div
              className="h-full rounded-full"
              style={{
                width: `${Math.max(2, (app.total_secs / max) * 100)}%`,
                background: appColor(app.app_key),
              }}
            />
          </div>
        </div>
      </button>
      {expanded && <AppTrend appId={app.application_id} appKey={app.app_key} />}
    </div>
  );
}

/** 展开区: 单应用近 30 天逐日迷你柱状 */
function AppTrend({ appId, appKey }: { appId: number; appKey: string }) {
  const [bars, setBars] = useState<Bar[] | null>(null);

  useEffect(() => {
    let alive = true;
    setBars(null);
    api
      .appTrend(appId, 30)
      .then((b) => alive && setBars(b))
      .catch(() => alive && setBars([]));
    return () => {
      alive = false;
    };
  }, [appId]);

  const max = Math.max(...(bars ?? []).map((b) => b.total_secs), 1);
  const total = (bars ?? []).reduce((acc, b) => acc + b.total_secs, 0);
  const color = appColor(appKey);

  return (
    <div className="page-enter border-t border-border-soft bg-surface/60 px-5 py-4">
      <div className="flex items-baseline justify-between text-[12.5px] text-muted">
        <span>近 30 天趋势</span>
        <span className="tabular-nums">合计 {formatDuration(total)}</span>
      </div>
      {bars === null ? (
        <div className="mt-3 text-[12.5px] text-faint">加载中…</div>
      ) : (
        <div className="mt-3 flex h-[64px] items-end gap-[2px]">
          {bars.map((b, i) => (
            <div
              key={i}
              title={`${b.label} · ${formatDuration(b.total_secs)}`}
              className={`flex-1 rounded-[2px] ${b.total_secs > 0 ? "app-trend-bar" : ""}`}
              style={{
                height: Math.max(3, (b.total_secs / max) * 64),
                background: color,
                opacity: i === bars.length - 1 ? 1 : 0.4,
              }}
            />
          ))}
        </div>
      )}
    </div>
  );
}
