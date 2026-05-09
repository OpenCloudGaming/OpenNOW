import type { AppAccentColor } from "@shared/gfn";

export interface AccentColorOption {
  value: AppAccentColor;
  labelKey: string;
  hex: `#${string}`;
}

const ACCENT_COLOR_OPTIONS: readonly AccentColorOption[] = [
  { value: "green", labelKey: "settings.interface.accentColorGreen", hex: "#58d98a" },
  { value: "blue", labelKey: "settings.interface.accentColorBlue", hex: "#4f8cff" },
  { value: "violet", labelKey: "settings.interface.accentColorViolet", hex: "#8b6cff" },
  { value: "amber", labelKey: "settings.interface.accentColorAmber", hex: "#f5b942" },
  { value: "rose", labelKey: "settings.interface.accentColorRose", hex: "#ff6b9a" },
] as const;

const ACCENT_COLOR_MAP = new Map<AppAccentColor, AccentColorOption>(
  ACCENT_COLOR_OPTIONS.map((option) => [option.value, option]),
);

interface RgbColor {
  r: number;
  g: number;
  b: number;
}

function clampByte(value: number): number {
  return Math.max(0, Math.min(255, Math.round(value)));
}

function hexToRgb(hex: string): RgbColor {
  const normalized = hex.replace("#", "");
  const expanded = normalized.length === 3
    ? normalized.split("").map((char) => `${char}${char}`).join("")
    : normalized;
  return {
    r: Number.parseInt(expanded.slice(0, 2), 16),
    g: Number.parseInt(expanded.slice(2, 4), 16),
    b: Number.parseInt(expanded.slice(4, 6), 16),
  };
}

function rgbToHex({ r, g, b }: RgbColor): `#${string}` {
  const hex = [r, g, b]
    .map((channel) => clampByte(channel).toString(16).padStart(2, "0"))
    .join("");
  return `#${hex}`;
}

function mixColors(base: RgbColor, target: RgbColor, amount: number): RgbColor {
  const mix = Math.max(0, Math.min(1, amount));
  return {
    r: clampByte(base.r + (target.r - base.r) * mix),
    g: clampByte(base.g + (target.g - base.g) * mix),
    b: clampByte(base.b + (target.b - base.b) * mix),
  };
}

function getContrastRatio(fg: RgbColor, bg: RgbColor): number {
  const relativeLuminance = (rgb: RgbColor): number => {
    const [rs, gs, bs] = [rgb.r / 255, rgb.g / 255, rgb.b / 255].map((channel) =>
      channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
    );
    return 0.2126 * rs + 0.7152 * gs + 0.0722 * bs;
  };
  const l1 = relativeLuminance(fg);
  const l2 = relativeLuminance(bg);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

function getContrastColor({ r, g, b }: RgbColor): string {
  const lightFg = { r: 248, g: 250, b: 252 };
  const darkFg = { r: 17, g: 19, b: 24 };
  const lightContrast = getContrastRatio(lightFg, { r, g, b });
  const darkContrast = getContrastRatio(darkFg, { r, g, b });
  return lightContrast > darkContrast ? "#f8fafc" : "#111318";
}

export function getAccentColorOptions(): readonly AccentColorOption[] {
  return ACCENT_COLOR_OPTIONS;
}

export function getAccentColorOption(accentColor: AppAccentColor): AccentColorOption {
  return ACCENT_COLOR_MAP.get(accentColor) ?? ACCENT_COLOR_OPTIONS[0];
}

export function applyAccentColor(accentColor: AppAccentColor, root: HTMLElement | null = null): void {
  const target = root ?? document.documentElement;
  const option = getAccentColorOption(accentColor);
  const baseRgb = hexToRgb(option.hex);
  const hoverRgb = mixColors(baseRgb, { r: 255, g: 255, b: 255 }, 0.12);
  const pressRgb = mixColors(baseRgb, { r: 0, g: 0, b: 0 }, 0.12);

  target.style.setProperty("--accent", option.hex);
  target.style.setProperty("--accent-hover", rgbToHex(hoverRgb));
  target.style.setProperty("--accent-press", rgbToHex(pressRgb));
  target.style.setProperty("--accent-rgb", `${baseRgb.r}, ${baseRgb.g}, ${baseRgb.b}`);
  target.style.setProperty("--accent-glow", `rgba(${baseRgb.r}, ${baseRgb.g}, ${baseRgb.b}, 0.25)`);
  target.style.setProperty("--accent-surface", `rgba(${baseRgb.r}, ${baseRgb.g}, ${baseRgb.b}, 0.08)`);
  target.style.setProperty("--accent-surface-strong", `rgba(${baseRgb.r}, ${baseRgb.g}, ${baseRgb.b}, 0.14)`);
  target.style.setProperty("--accent-on", getContrastColor(baseRgb));
}
