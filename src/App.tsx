import { useState } from "react";
import { Sidebar, type PageId } from "./components/Sidebar";
import { Overview } from "./pages/Overview";
import { Apps } from "./pages/Apps";
import { Settings } from "./pages/Settings";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import { DateSelector } from "./components/DateSelector";
import { SegmentedControl } from "./components/SegmentedControl";

type Period = "today" | "7d";

export default function App() {
  const [page, setPage] = useState<PageId>("overview");
  const [period, setPeriod] = useState<Period>("today");

  return (
    <div className="app-shell">
      <header className="titlebar" data-tauri-drag-region>
        <span className="titlebar-name" data-tauri-drag-region>屏幕时间</span>
        {page === "overview" && <div className="titlebar-filters"><DateSelector range={period} /><SegmentedControl options={[{ value: "7d", label: "每周" }, { value: "today", label: "今天" }]} value={period} onChange={setPeriod} /></div>}
        <div className="window-controls">
          <button aria-label="最小化" title="最小化" onClick={() => void getCurrentWindow().minimize()}><Minus size={15} /></button>
          <button aria-label="最大化或还原" title="最大化或还原" onClick={() => void getCurrentWindow().toggleMaximize()}><Square size={12} /></button>
          <button aria-label="关闭窗口" title="关闭窗口" onClick={() => void getCurrentWindow().close()}><X size={15} /></button>
        </div>
      </header>
      <div className="app-content">
        <Sidebar active={page} onSelect={setPage} />
        <main key={page} className="main-content">
          {page === "overview" && <Overview period={period} onOpenApps={() => setPage("apps")} />}
          {page === "apps" && <Apps />}
          {page === "settings" && <Settings />}
        </main>
      </div>
    </div>
  );
}
