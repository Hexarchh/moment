/** 克制的开关 (150ms 滑块过渡) */
export function Toggle({
  on,
  disabled,
  onClick,
}: {
  on: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-pressed={on}
      aria-label="开机自动启动"
      className={`relative h-[20px] w-[36px] shrink-0 rounded-full transition-colors duration-150 ${
        on ? "bg-accent" : "bg-border"
      } ${disabled ? "opacity-40" : ""}`}
    >
      <span
        className={`absolute left-[2px] top-[2px] h-[16px] w-[16px] rounded-full bg-white transition-transform duration-150 ${
          on ? "translate-x-[16px]" : "translate-x-0"
        }`}
      />
    </button>
  );
}
