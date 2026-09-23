export function DateSelector({ range }: { range: "today" | "7d" }) {
  const today = new Date();
  const date = new Intl.DateTimeFormat("zh-CN", { month: "long", day: "numeric" }).format(today);
  return <span className="date-label">{range === "today" ? `${date} · 今天` : `截至 ${date} · 近 7 天`}</span>;
}
