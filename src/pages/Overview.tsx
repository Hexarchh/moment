import { useEffect, useState } from "react";
import { AppUsageList } from "../components/AppUsageList";
import { CategorySummary, categoryTotals } from "../components/CategorySummary";
import { HourlyActivityChart } from "../components/HourlyActivityChart";
import { ScreenTimeSummary } from "../components/ScreenTimeSummary";
import { WeeklyUsageChart } from "../components/WeeklyUsageChart";
import { TodayPlanCard } from "../components/TodayPlanCard";
import { ActivityGraph } from "../components/ActivityGraph";
import { api, formatDuration, type Overview as OverviewData } from "../lib/api";

type Period = "today" | "7d";

export function Overview({ period, onOpenApps, onOpenPlan }: { period: Period; onOpenApps: () => void; onOpenPlan: () => void }) {
  const [data, setData] = useState<OverviewData | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    let active = true;
    setData(null);
    setError(false);
    const load = () => api.overview(period).then((result) => {
      if (!active) return;
      setData(result);
      setError(false);
    }).catch(() => {
      if (active) setError(true);
    });
    void load();
    const timer = window.setInterval(() => void load(), 20_000);
    const onFocus = () => void load();
    window.addEventListener("focus", onFocus);
    return () => {
      active = false;
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [period]);

  const apps = data?.top_apps ?? [];
  const weeklyBars = data?.weekly_bars ?? [];
  const categoryApps = data?.category_apps ?? [];
  const dailyAverage = data && period === "7d" ? data.total_secs / 7 : 0;

  return (
    <div className="overview-page page-enter">
      <header className="overview-header">
        <h1>概览</h1>
      </header>

      {data?.paused && <div className="paused-note">统计已暂停，使用时间暂不累计。</div>}
      {error && !data && <div className="paused-note">暂时无法读取使用记录，请稍后重试。</div>}

      {period === "today" && <TodayPlanCard onOpenPlan={onOpenPlan} />}

      <div className="overview-grid">
        <section className="screen-time-card" aria-label="屏幕时间统计">
          <ScreenTimeSummary total={data?.total_secs ?? 0} compare={data?.compare_secs ?? 0} range={period} loading={!data} />
          {period === "7d" && <div className="daily-average"><span>日均使用</span><strong>{data ? formatDuration(dailyAverage) : "—"}</strong></div>}
          <div className="chart-section"><WeeklyUsageChart bars={weeklyBars} todayCategories={period === "today" ? categoryTotals(categoryApps) : undefined} /></div>
          {period === "today" && <div className="chart-section"><HourlyActivityChart bars={data?.bars ?? []} /></div>}
          <CategorySummary apps={categoryApps} />
        </section>
        <AppUsageList apps={apps} loading={!data && !error} onAppClick={onOpenApps} />
      </div>

      <ActivityGraph />
    </div>
  );
}
