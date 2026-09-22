import { useCallback, useEffect, useState } from "react";
import { AppIcon } from "../components/AppIcon";
import {
  api,
  appColor,
  deltaPercent,
  formatDuration,
  type Overview as OverviewData,
  type Range,
} from "../lib/api";

const RANGES: { id: Range; label: string }[] = [
  { id: "today", label: "今日" },
  { id: "7d", label: "近 7 天" },
  { id: "30d", label: "近 30 天" },
];

export function Overview() {
  const [range, setRange] = useState<Range>("today");
  const [data, setData] = useState<OverviewData | null>(null);

  const load = useCallback(
    (r: Range) => {
      api.overview(r).then(setData).catch(() => {});
    },
    [],
  );

  useEffect(() => {
    load(range);
    const t = setInterval(() => load(range), 20_000);
    const onFocus = () => load(range);
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(t);
      window.removeEventListener("focus", onFocus);
    };
  }, [load, range]);

  const bars = data?.bars ?? [];
  const barMax = Math.max(...bars.map((b) => b.total_secs), 1);
  const topApps = data?.top_apps ?? [];
  const topMax = Math.max(...topApps.map((a) => a.total_secs), 1);
  const delta = data ? deltaPercent(data.total_secs, data.compare_secs) : null;

  // 柱状图下方标签稀疏化
  const labelStep = bars.length >= 30 ? 5 : bars.length >= 24 ? 6 : 1;

  return (
    <div className="page-enter mx-auto max-w-[860px] px-10 py-9">
      <header className="flex items-center justify-between">
        <h1 className="text-[22px] font-semibold tracking-tight">概览</h1>
        <div className="flex overflow-hidden rounded-control border border-border">
          {RANGES.map((r) => (
            <button
              key={r.id}
              onClick={() => setRange(r.id)}
              className={`px-3 py-1 text-[12.5px] transition-colors duration-150 ${
                range === r.id ? "bg-card text-text" : "text-muted hover:text-text"
              }`}
            >
              {r.label}
            </button>
          ))}
        </div>
      </header>

      {data?.paused && (
        <div className="mt-5 rounded-control border border-border bg-card px-4 py-2.5 text-[13px] text-muted">
          统计已暂停，时间不再累计。
        </div>
      )}

      <section className="mt-8">
        <div className="flex items-baseline gap-4">
          <div className="text-[44px] font-semibold leading-none tracking-tight tabular-nums">
            {data ? formatDuration(data.total_secs) : "—"}
          </div>
          {delta !== null && (
            <span
              className="text-[13px] tabular-nums"
              style={{ color: delta >= 0 ? "#e0823d" : "#4cb782" }}
            >
              {delta >= 0 ? "↑" : "↓"} {Math.abs(delta).toFixed(0)}%
              <span className="ml-1 text-faint">
                {range === "today" ? "较昨日" : "较上一周期"}
              </span>
            </span>
          )}
        </div>
        <div className="mt-2 text-[13px] text-muted">
          {range === "today" ? "今日屏幕总时间" : range === "7d" ? "近 7 天屏幕总时间" : "近 30 天屏幕总时间"}
        </div>
      </section>

      <section className="mt-10 rounded-card border border-border bg-card p-6">
        <div className="text-[13px] font-medium text-muted">
          {range === "today" ? "今日逐时活跃" : "每日活跃"}
        </div>
        <div className="mt-5 flex h-[120px] items-end gap-[3px]">
          {bars.map((b, i) => (
            <div
              key={i}
              title={`${b.label} · ${formatDuration(b.total_secs)}`}
              className="flex-1 rounded-[3px] bg-accent/70 transition-[height] duration-300"
              style={{
                height: Math.max(4, (b.total_secs / barMax) * 120),
                opacity: i === bars.length - 1 && range !== "today" ? 1 : 0.45,
              }}
            />
          ))}
        </div>
        <div className="mt-2.5 flex gap-[3px] text-[10px] text-faint">
          {bars.map((b, i) => (
            <div key={i} className="flex-1 text-center">
              {i % labelStep === 0 || i === bars.length - 1
                ? range === "7d"
                  ? b.label.split(" ")[0].replace("周", "")
                  : b.label
                : ""}
            </div>
          ))}
        </div>
      </section>

      <section className="mt-8">
        <div className="text-[13px] font-medium text-muted">最常用应用</div>
        {topApps.length === 0 ? (
          <div className="mt-4 rounded-card border border-border bg-card px-5 py-6 text-center text-[13px] text-faint">
            从第一次应用切换开始统计。
          </div>
        ) : (
          <div className="mt-4 overflow-hidden rounded-card border border-border bg-card">
            {topApps.map((a) => {
              const pct =
                data && data.total_secs > 0
                  ? Math.round((a.total_secs / data.total_secs) * 100)
                  : 0;
              return (
                <div
                  key={a.application_id}
                  className="border-b border-border-soft px-5 py-3 last:border-b-0"
                >
                  <div className="flex items-baseline justify-between">
                    <div className="flex items-center gap-2.5 text-[13.5px]">
                      <AppIcon appKey={a.app_key} name={a.name} />
                      <span className="font-medium">{a.name}</span>
                      {pct > 0 && (
                        <span className="text-[12px] tabular-nums text-faint">{pct}%</span>
                      )}
                    </div>
                    <span className="text-[13px] tabular-nums text-muted">
                      {formatDuration(a.total_secs)}
                    </span>
                  </div>
                  <div className="mt-2 h-[3px] overflow-hidden rounded-full bg-border">
                    <div
                      className="h-full rounded-full transition-[width] duration-300"
                      style={{
                        width: `${Math.max(2, (a.total_secs / topMax) * 100)}%`,
                        background: appColor(a.app_key),
                      }}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}
