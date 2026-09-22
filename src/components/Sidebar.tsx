import { LayoutGrid, AppWindow, Settings } from "lucide-react";

export type PageId = "overview" | "apps" | "settings";

const NAV = [
  { id: "overview", label: "概览", icon: LayoutGrid },
  { id: "apps", label: "应用", icon: AppWindow },
  { id: "settings", label: "设置", icon: Settings },
] as const;

export function Sidebar({
  active,
  onSelect,
}: {
  active: PageId;
  onSelect: (p: PageId) => void;
}) {
  return (
    <aside className="flex h-full w-[220px] shrink-0 flex-col border-r border-border bg-surface">
      <div className="flex items-center gap-2.5 px-5 pb-2 pt-5">
        <div className="h-[16px] w-[16px] rounded-full border-2 border-accent" />
        <span className="text-[15px] font-semibold tracking-tight">Moment</span>
      </div>

      <nav className="mt-3 flex flex-col gap-0.5 px-3">
        {NAV.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => onSelect(id)}
            className={`flex items-center gap-2.5 rounded-control px-3 py-[7px] text-left text-[13.5px] transition-colors duration-150 ${
              active === id
                ? "bg-card text-text"
                : "text-muted hover:bg-card/60 hover:text-text"
            }`}
          >
            <Icon size={16} strokeWidth={1.75} />
            {label}
          </button>
        ))}
      </nav>

      <div className="mt-auto px-5 pb-4 text-[11px] text-faint">
        v0.1.0 · 本地优先
      </div>
    </aside>
  );
}
