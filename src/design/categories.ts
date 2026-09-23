import type { AppUsage } from "../lib/api";
import { colors } from "./colors";

export const categories = [
  { id: "social", label: "社交", color: colors.social },
  { id: "productivity", label: "效率与创作", color: colors.productivity },
  { id: "entertainment", label: "娱乐", color: colors.entertainment },
  { id: "reading", label: "信息与阅读", color: colors.reading },
  { id: "other", label: "其他", color: colors.other },
] as const;

export type CategoryId = (typeof categories)[number]["id"];

/** 只识别明确的应用标识；未知应用始终归入「其他」。 */
export function categoryFor(app: Pick<AppUsage, "app_key" | "name">): CategoryId {
  const key = `${app.app_key} ${app.name}`.toLowerCase();
  if (/wechat|weixin|telegram|discord|slack|signal|whatsapp|\bqq\b|teams/.test(key)) return "social";
  if (/code|codium|idea|pycharm|webstorm|terminal|konsole|alacritty|kitty|word|excel|office|libreoffice|figma|photoshop|blender|obsidian|notion/.test(key)) return "productivity";
  if (/spotify|music|steam|game|vlc|bilibili|netflix|youtube/.test(key)) return "entertainment";
  if (/reader|book|kindle|zotero/.test(key)) return "reading";
  return "other";
}

export function categoryColor(app: Pick<AppUsage, "app_key" | "name">): string {
  return categories.find((category) => category.id === categoryFor(app))!.color;
}
