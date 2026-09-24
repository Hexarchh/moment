import { useCallback, useEffect, useRef, useState } from "react";
import { Check } from "lucide-react";
import {
  api,
  formatDuration,
  formatMinutes,
  localDateString,
  type PlanTask,
} from "../lib/api";

const WEEKDAY_ZH = ["日", "一", "二", "三", "四", "五", "六"];

/** date (YYYY-MM-DD) → 9月24日 */
function dateNumber(date: string): string {
  const d = new Date(`${date}T00:00:00`);
  return `${d.getMonth() + 1}月${d.getDate()}日`;
}

function dateLabel(offsetDays: number): string {
  if (offsetDays === 0) return "今天";
  if (offsetDays === 1) return "昨天";
  return dateNumber(localDateString(offsetDays));
}

export function DailyPlan() {
  const [offset, setOffset] = useState(0);
  const [tasks, setTasks] = useState<PlanTask[] | null>(null);
  const [adding, setAdding] = useState(false);
  const [screenSecs, setScreenSecs] = useState<number | null>(null);

  const date = localDateString(offset);
  const isToday = offset === 0;

  useEffect(() => {
    let alive = true;
    setTasks(null);
    api
      .planTasks(date)
      .then((t) => alive && setTasks(t))
      .catch(() => alive && setTasks([]));
    return () => {
      alive = false;
    };
  }, [date]);

  // 今日小结联动屏幕时间 (只读一次, 无轮询)
  useEffect(() => {
    if (!isToday) {
      setScreenSecs(null);
      return;
    }
    let alive = true;
    api
      .overview("today")
      .then((o) => alive && setScreenSecs(o.total_secs))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [isToday]);

  const replace = useCallback((task: PlanTask) => {
    setTasks((prev) =>
      (prev ?? []).map((t) => (t.id === task.id ? task : t)),
    );
  }, []);

  const remove = useCallback((id: number) => {
    setTasks((prev) => (prev ?? []).filter((t) => t.id !== id));
    api.planDelete(id).catch(() => {});
  }, []);

  const add = useCallback(
    (title: string, estimated: number | null) => {
      api
        .planAdd(date, title, estimated, null)
        .then((task) => setTasks((prev) => [...(prev ?? []), task]))
        .catch(() => {});
    },
    [date],
  );

  const toggle = useCallback(
    (task: PlanTask) => {
      const next = !task.completed;
      replace({ ...task, completed: next });
      api
        .planToggle(task.id, next)
        .then(replace)
        .catch(() => replace(task));
    },
    [replace],
  );

  const list = tasks ?? [];
  const done = list.filter((t) => t.completed).length;
  const plannedMinutes = list.reduce(
    (acc, t) => acc + (t.estimated_minutes ?? 0),
    0,
  );
  const doneMinutes = list
    .filter((t) => t.completed)
    .reduce((acc, t) => acc + (t.estimated_minutes ?? 0), 0);
  const weekday = WEEKDAY_ZH[new Date(`${date}T00:00:00`).getDay()];

  return (
    <div className="plan-page page-enter">
      <header className="plan-header">
        <div>
          <h1>每日计划</h1>
          <span className="date-label plan-stats">
            {dateNumber(date)} · 周{weekday}
            {list.length > 0 && ` · ${list.length} 项任务 · ${done} 已完成`}
          </span>
        </div>
        <div className="plan-nav">
          <button type="button" aria-label="前一天" disabled={offset >= 365} onClick={() => setOffset(offset + 1)}>
            ‹
          </button>
          <button
            type="button"
            className="plan-date-chip"
            disabled={isToday}
            title="回到今天"
            onClick={() => setOffset(0)}
          >
            {dateLabel(offset)}
          </button>
          <button type="button" aria-label="后一天" disabled={isToday} onClick={() => setOffset(offset - 1)}>
            ›
          </button>
        </div>
      </header>

      {tasks === null ? (
        <div className="plan-list">
          <p className="usage-empty">加载中…</p>
        </div>
      ) : list.length === 0 && !adding ? (
        <div className="plan-list">
          <div className="plan-empty">
            <h2>还没有计划</h2>
            <p>{isToday ? "今天想完成什么？" : "这一天没有记录任何计划。"}</p>
            <button type="button" onClick={() => setAdding(true)}>
              ＋ 添加第一个任务
            </button>
          </div>
        </div>
      ) : (
        <div className="plan-list">
          {list.map((task) => (
            <TaskRow
              key={task.id}
              task={task}
              onToggle={() => toggle(task)}
              onSave={(title, estimated) =>
                api
                  .planSave(task.id, title, estimated, task.note)
                  .then(replace)
                  .catch(() => {})
              }
              onDelete={() => remove(task.id)}
            />
          ))}
          {adding ? (
            <AddRow
              onCancel={() => setAdding(false)}
              onAdd={(title, estimated) => add(title, estimated)}
            />
          ) : (
            <button type="button" className="plan-add" onClick={() => setAdding(true)}>
              <span className="plan-add-mark">＋</span>添加任务
            </button>
          )}
        </div>
      )}

      {tasks !== null && list.length > 0 && (
        <p className="plan-summary">
          计划 {plannedMinutes > 0 ? formatMinutes(plannedMinutes) : "—"}
          {done > 0 && ` · 已完成 ${done} 项`}
          {doneMinutes > 0 && `（约 ${formatMinutes(doneMinutes)}）`}
          {isToday && screenSecs !== null && ` · 屏幕时间 ${formatDuration(screenSecs)}`}
        </p>
      )}
    </div>
  );
}

/** 单行任务: 圆形勾选 + 标题(点击编辑) + hover 出 ⋯ 菜单 */
function TaskRow({
  task,
  onToggle,
  onSave,
  onDelete,
}: {
  task: PlanTask;
  onToggle: () => void;
  onSave: (title: string, estimatedMinutes: number | null) => void;
  onDelete: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);

  if (editing) {
    return (
      <EditRow
        task={task}
        onCancel={() => setEditing(false)}
        onSave={(title, estimated) => {
          onSave(title, estimated);
          setEditing(false);
        }}
      />
    );
  }

  return (
    <div
      className={`plan-task${task.completed ? " done" : ""}`}
      onKeyDown={(e) => {
        if (e.key === "Delete" && !menuOpen) onDelete();
      }}
      tabIndex={-1}
    >
      <button
        type="button"
        className={`plan-check${task.completed ? " done" : ""}`}
        aria-pressed={task.completed}
        aria-label={task.completed ? "标记为未完成" : "标记为已完成"}
        onClick={onToggle}
      >
        <Check size={13} strokeWidth={3} />
      </button>
      <div className="plan-body">
        <button type="button" className="plan-title" onClick={() => setEditing(true)} title="点击编辑">
          {task.title}
        </button>
        {(task.estimated_minutes != null || task.note) && (
          <div className="plan-meta">
            {task.estimated_minutes != null && (
              <span>预计 {formatMinutes(task.estimated_minutes)}</span>
            )}
            {task.note && <span className="plan-note">{task.note}</span>}
          </div>
        )}
      </div>
      <button
        type="button"
        className={`plan-more${menuOpen ? " open" : ""}`}
        aria-label="更多操作"
        onClick={() => setMenuOpen(!menuOpen)}
        onBlur={() => setMenuOpen(false)}
      >
        ⋯
      </button>
      {menuOpen && (
        <div className="plan-menu" onMouseDown={(e) => e.preventDefault()}>
          <button
            type="button"
            onClick={() => {
              setMenuOpen(false);
              setEditing(true);
            }}
          >
            编辑
          </button>
          <button
            type="button"
            className="danger"
            onClick={() => {
              setMenuOpen(false);
              onDelete();
            }}
          >
            删除
          </button>
        </div>
      )}
    </div>
  );
}

/** 行内编辑: 标题 + 预计分钟; Enter 保存, Esc 取消 */
function EditRow({
  task,
  onCancel,
  onSave,
}: {
  task: PlanTask;
  onCancel: () => void;
  onSave: (title: string, estimatedMinutes: number | null) => void;
}) {
  const [title, setTitle] = useState(task.title);
  const [estimated, setEstimated] = useState(
    task.estimated_minutes != null ? String(task.estimated_minutes) : "",
  );
  const [note, setNote] = useState(task.note ?? "");
  const titleRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    titleRef.current?.focus();
    titleRef.current?.select();
  }, []);

  const commit = () => {
    const t = title.trim();
    if (!t) {
      onCancel();
      return;
    }
    const mins = Number(estimated);
    onSave(t, Number.isFinite(mins) && mins > 0 ? Math.min(mins, 1440) : null);
  };

  return (
    <div className="plan-task">
      <span className={`plan-check${task.completed ? " done" : ""}`} aria-hidden="true" />
      <div className="plan-body plan-edit-grid">
        <input
          ref={titleRef}
          className="plan-input"
          value={title}
          placeholder="任务标题"
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit();
            if (e.key === "Escape") onCancel();
          }}
        />
        <input
          className="plan-input"
          value={estimated}
          placeholder="分钟"
          inputMode="numeric"
          onChange={(e) => setEstimated(e.target.value.replace(/[^\d]/g, ""))}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit();
            if (e.key === "Escape") onCancel();
          }}
        />
        <input
          className="plan-input"
          value={note}
          placeholder="备注"
          onChange={(e) => setNote(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit();
            if (e.key === "Escape") onCancel();
          }}
        />
      </div>
    </div>
  );
}

/** 连续录入行: Enter 建并保持焦点, Esc 或空值失焦退出 */
function AddRow({
  onAdd,
  onCancel,
}: {
  onAdd: (title: string, estimatedMinutes: number | null) => void;
  onCancel: () => void;
}) {
  const [title, setTitle] = useState("");
  const [estimated, setEstimated] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const create = (): boolean => {
    const t = title.trim();
    if (!t) return false;
    const mins = Number(estimated);
    onAdd(t, Number.isFinite(mins) && mins > 0 ? Math.min(mins, 1440) : null);
    setTitle("");
    setEstimated("");
    return true;
  };

  return (
    <div className="plan-add-input">
      <span className="plan-add-mark" aria-hidden="true">＋</span>
      <div className="plan-edit-grid">
        <input
          ref={inputRef}
          className="plan-input"
          value={title}
          placeholder="输入今天要做的事情…"
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") create();
            if (e.key === "Escape") onCancel();
          }}
        />
        <input
          className="plan-input"
          value={estimated}
          placeholder="分钟"
          inputMode="numeric"
          title="预计时间 (分钟, 可选)"
          onChange={(e) => setEstimated(e.target.value.replace(/[^\d]/g, ""))}
          onKeyDown={(e) => {
            if (e.key === "Enter") create();
            if (e.key === "Escape") onCancel();
          }}
        />
        <button
          type="button"
          className="plan-input"
          style={{ cursor: "pointer", color: "var(--color-muted)" }}
          title="完成添加"
          onClick={() => {
            if (!create()) onCancel();
          }}
        >
          保存
        </button>
      </div>
      <button type="button" className="plan-more" style={{ opacity: 1 }} aria-label="取消添加" onClick={onCancel}>
        ×
      </button>
    </div>
  );
}
