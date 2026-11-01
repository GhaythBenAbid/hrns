import { Platform, StyleSheet } from "react-native";

/** Menlo exists only on iOS; Android needs its own monospace family. */
const MONO = Platform.OS === "ios" ? "Menlo" : "monospace";

// Same palettes and accent presets as the desktop app.
const dark = {
  bg: "#08080a",
  panel: "#111113",
  raised: "#18181b",
  hover: "#1c1c20",
  border: "#232327",
  text: "#e8e8ec",
  muted: "#85858f",
  faint: "#55555d",
  accent: "#a78bfa",
  accentInk: "#0b0814",
  bubble: "#1a1a1d",
  danger: "#f87171",
  dangerBg: "#1f0f10",
  green: "#4ade80",
  amber: "#fbbf24",
  codeBg: "#0b0b0d",
  codeText: "#d4d4d8",
  tabBg: "#0b0b0d",
  waitBorder: "#5a4516",
  waitBg: "#16130c",
  mono: MONO,
};
type Palette = typeof dark;

const light: Palette = {
  bg: "#ffffff",
  panel: "#ffffff",
  raised: "#f2f2f4",
  hover: "#ececef",
  border: "#e4e4e8",
  text: "#18181b",
  muted: "#6b6b76",
  faint: "#a1a1aa",
  accent: "#7c3aed",
  accentInk: "#ffffff",
  bubble: "#f2f2f4",
  danger: "#dc2626",
  dangerBg: "#fef2f2",
  green: "#16a34a",
  amber: "#d97706",
  codeBg: "#f7f7f8",
  codeText: "#27272a",
  tabBg: "#f8f8f9",
  waitBorder: "#f1d08a",
  waitBg: "#fffbeb",
  mono: MONO,
};

export const accents = {
  violet: { label: "Violet", dark: "#a78bfa", light: "#7c3aed" },
  blue: { label: "Blue", dark: "#60a5fa", light: "#2563eb" },
  teal: { label: "Teal", dark: "#2dd4bf", light: "#0d9488" },
  green: { label: "Green", dark: "#4ade80", light: "#16a34a" },
  amber: { label: "Amber", dark: "#fbbf24", light: "#d97706" },
  orange: { label: "Orange", dark: "#fb923c", light: "#ea580c" },
  rose: { label: "Rose", dark: "#fb7185", light: "#e11d48" },
  pink: { label: "Pink", dark: "#f472b6", light: "#db2777" },
} as const;
export type Accent = keyof typeof accents;
export type ThemeMode = "system" | "light" | "dark";
export type Appearance = { theme?: ThemeMode; accent?: Accent };

/**
 * The live palette. Components read it at render time; App remounts the tree when it changes,
 * so every screen picks up the new colors without threading a theme through props.
 */
export const c: Palette & { scheme: "light" | "dark" } = { ...dark, scheme: "dark" };

let version = 0;

/** Switch palettes. Returns true when anything changed. */
export function applyTheme(scheme: "light" | "dark", accent: Accent = "violet") {
  const next = { ...(scheme === "light" ? light : dark), scheme };
  next.accent = accents[accent]?.[scheme] ?? next.accent;
  // Light accents are dark enough for white text; dark-mode accents need dark ink.
  next.accentInk = scheme === "light" ? "#ffffff" : "#0b0814";
  if ((Object.keys(next) as (keyof typeof next)[]).every((k) => c[k] === next[k])) return false;
  Object.assign(c, next);
  version++;
  return true;
}

/** Translucent version of a #rrggbb color (for tinted backgrounds and borders). */
export function alpha(hex: string, a: number) {
  const n = Math.round(Math.max(0, Math.min(1, a)) * 255).toString(16).padStart(2, "0");
  return hex.length === 7 ? hex + n : hex;
}

/**
 * StyleSheet.create, rebuilt lazily from the current palette after each theme change.
 * Use like StyleSheet.create, but pass a function: `const styles = sheet(() => ({ ... }))`.
 */
type Styles = Parameters<typeof StyleSheet.create>[0];
export function sheet<T extends Styles>(make: () => T & Styles): Readonly<T> {
  let built: Readonly<T> | null = null;
  let at = -1;
  return new Proxy({} as Readonly<T>, {
    get(_, key) {
      if (at !== version || !built) {
        built = StyleSheet.create(make()) as Readonly<T>;
        at = version;
      }
      return built[key as keyof T];
    },
  });
}

/** Like `sheet`, but returns a plain object (for libraries that enumerate style keys). */
export function themed<T>(make: () => T): () => T {
  let built: T | null = null;
  let at = -1;
  return () => {
    if (at !== version || built === null) {
      built = make();
      at = version;
    }
    return built;
  };
}
