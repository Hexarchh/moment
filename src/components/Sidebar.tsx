import { ChartNoAxesColumn, ListChecks, AppWindow, Settings } from "lucide-react";

export type PageId = "overview" | "plan" | "apps" | "settings";

const NAV = [
  { id: "overview", label: "概览", icon: ChartNoAxesColumn },
  { id: "plan", label: "每日计划", icon: ListChecks },
  { id: "apps", label: "应用", icon: AppWindow },
] as const;

export function Sidebar({ active, onSelect }: { active: PageId; onSelect: (page: PageId) => void }) {
  return (
    <aside className="sidebar">
      <nav className="sidebar-nav" aria-label="主导航">
        {NAV.map(({ id, label, icon: Icon }) => <button key={id} type="button" className={`nav-item ${active === id ? "active" : ""}`} onClick={() => onSelect(id)} aria-current={active === id ? "page" : undefined}><Icon size={17} strokeWidth={1.75} /><span>{label}</span></button>)}
      </nav>
      <button type="button" className={`nav-item sidebar-settings ${active === "settings" ? "active" : ""}`} onClick={() => onSelect("settings")} aria-current={active === "settings" ? "page" : undefined}><Settings size={17} strokeWidth={1.75} /><span>设置</span></button>
    </aside>
  );
}
