const SCALE_STEPS = [
  300, 600, 900, 1200, 1800, 2700, 3600, 5400,
  7200, 10800, 14400, 21600, 28800, 43200, 86400,
];

/** Choose a round time boundary so the grid lines have meaningful labels. */
export function usageChartMax(values: number[]): number {
  const highest = Math.max(0, ...values);
  return SCALE_STEPS.find((step) => step >= highest) ?? Math.ceil(highest / 3600) * 3600;
}

function axisDuration(secs: number): string {
  if (secs === 0) return "0";
  if (secs >= 3600) return `${Number((secs / 3600).toFixed(1))}h`;
  return `${Number((secs / 60).toFixed(1))}m`;
}

export function UsageChartAxis({ max }: { max: number }) {
  return (
    <div className="chart-axis" aria-hidden="true">
      <span>{axisDuration(max)}</span>
      <span>{axisDuration(max / 2)}</span>
      <span>0</span>
    </div>
  );
}
