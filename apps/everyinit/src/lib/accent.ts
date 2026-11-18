export interface AccentPreset {
  id: string;
  label: string;
  accent: string;
  strong: string;
}

export const ACCENT_PRESETS: readonly AccentPreset[] = [
  { id: "lime", label: "Lime", accent: "#d4f36a", strong: "#4a6b1d" },
  { id: "mint", label: "Mint", accent: "#8ce6b8", strong: "#1e6b48" },
  { id: "sky", label: "Sky", accent: "#8ad8f5", strong: "#1c5f7a" },
  { id: "amber", label: "Amber", accent: "#f6cf65", strong: "#7a590c" },
  { id: "apricot", label: "Apricot", accent: "#f7b277", strong: "#854516" },
  { id: "lavender", label: "Lavender", accent: "#c8b6f7", strong: "#4f3d85" },
  { id: "rose", label: "Rose", accent: "#f3a6b8", strong: "#822d44" },
];

export function getDailyAccent(date: Date = new Date()): AccentPreset {
  return ACCENT_PRESETS[date.getDay()] ?? ACCENT_PRESETS[0];
}

export function getLocalDayKey(date: Date = new Date()): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}
