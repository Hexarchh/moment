import { useEffect, useState } from "react";
import { appColor, appIconUrl } from "../lib/api";

/** 应用图标: 优先真实图标 (data URL), 失败回退首字母色块 */
export function AppIcon({
  appKey,
  name,
  size = 22,
}: {
  appKey: string;
  name: string;
  size?: number;
}) {
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    setFailed(false);
    appIconUrl(appKey).then((u) => alive && setUrl(u));
    return () => {
      alive = false;
    };
  }, [appKey]);

  if (url && !failed) {
    return (
      <img
        src={url}
        alt=""
        width={size}
        height={size}
        className="shrink-0 rounded-[5px]"
        style={{ objectFit: "contain" }}
        onError={() => setFailed(true)}
      />
    );
  }
  return (
    <span
      className="flex shrink-0 items-center justify-center rounded-[5px] text-[11px] font-semibold uppercase text-text"
      style={{ width: size, height: size, background: `${appColor(appKey)}2e` }}
    >
      {name.slice(0, 1)}
    </span>
  );
}
