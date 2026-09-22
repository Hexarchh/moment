import { useState } from "react";
import { Sidebar, type PageId } from "./components/Sidebar";
import { Overview } from "./pages/Overview";
import { Apps } from "./pages/Apps";
import { Settings } from "./pages/Settings";

export default function App() {
  const [page, setPage] = useState<PageId>("overview");

  return (
    <div className="flex h-screen overflow-hidden bg-bg text-text">
      <Sidebar active={page} onSelect={setPage} />
      <main key={page} className="flex-1 overflow-y-auto">
        {page === "overview" && <Overview />}
        {page === "apps" && <Apps />}
        {page === "settings" && <Settings />}
      </main>
    </div>
  );
}
