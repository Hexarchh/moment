import { useEffect, useState } from "react";
import { api, localDateString, type PlanTask } from "../lib/api";

/** 首页「今日计划」小卡: 克制地展示今天准备做什么, 点击跳转每日计划页 */
export function TodayPlanCard({ onOpenPlan }: { onOpenPlan: () => void }) {
  const [tasks, setTasks] = useState<PlanTask[] | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .planTasks(localDateString(0))
      .then((t) => alive && setTasks(t))
      .catch(() => alive && setTasks(null));
    return () => {
      alive = false;
    };
  }, []);

  if (tasks === null || tasks.length === 0) return null;

  const done = tasks.filter((t) => t.completed).length;
  const visible = [...tasks].sort((a, b) => Number(a.completed) - Number(b.completed)).slice(0, 4);

  return (
    <section className="today-plan" aria-label="今日计划">
      <div className="today-plan-head">
        <h2>今日计划</h2>
        <span>{done} / {tasks.length} 已完成</span>
      </div>
      {visible.map((task) => (
        <div key={task.id} className={`today-plan-item${task.completed ? " done" : ""}`}>
          <i aria-hidden="true" />
          <span>{task.title}</span>
        </div>
      ))}
      <button type="button" className="today-plan-more" onClick={onOpenPlan}>
        {tasks.length > visible.length ? `还有 ${tasks.length - visible.length} 项 · ` : ""}
        查看全部 ›
      </button>
    </section>
  );
}
