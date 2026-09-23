import { formatDuration, type AppUsage } from "../lib/api";
import { categories, categoryFor, type CategoryId } from "../design/categories";

export function categoryTotals(apps: AppUsage[]): Partial<Record<CategoryId, number>> {
  const totals: Partial<Record<CategoryId, number>> = {};
  for (const app of apps) {
    const category = categoryFor(app);
    totals[category] = (totals[category] ?? 0) + app.total_secs;
  }
  return totals;
}

export function CategorySummary({ apps }: { apps: AppUsage[] }) {
  const totals = categoryTotals(apps);
  const ordered = categories.filter((category) => (totals[category.id] ?? 0) > 0).sort((a, b) => (totals[b.id] ?? 0) - (totals[a.id] ?? 0));
  return (
    <div className="category-summary">
      {ordered.length ? ordered.map((category) => (
        <div className="category-stat" key={category.id}>
          <span className="category-name"><i style={{ background: category.color }} />{category.label}</span>
          <strong>{formatDuration(totals[category.id] ?? 0)}</strong>
        </div>
      )) : <span className="category-empty">使用应用后，这里会显示分类统计。</span>}
    </div>
  );
}
