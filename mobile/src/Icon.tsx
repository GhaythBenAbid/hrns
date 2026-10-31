import Svg, { Circle, Path, Rect } from "react-native-svg";

// Line icons matching the desktop app (24×24 grid, round caps).
const PATHS = {
  chat: "M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12Z",
  usage: "M5 20V12M12 20V5M19 20v-9",
  settings: "M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z",
  bolt: "M13 2 3 14h9l-1 8 10-12h-9Z",
  plus: "M12 5v14M5 12h14",
  x: "m9 9 6 6M15 9l-6 6",
  alert: "M12 8v5M12 16.5v.5",
  chevronRight: "m9 6 6 6-6 6",
  chevronDown: "m6 9 6 6 6-6",
  back: "M15 18l-6-6 6-6",
  send: "M12 19V5M5 12l7-7 7 7",
  bell: "M6 8a6 6 0 1 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10.3 21a1.94 1.94 0 0 0 3.4 0",
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z",
  search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14ZM20 20l-3.6-3.6",
  branch: "M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM18 9a9 9 0 0 1-9 9",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 18, color, strokeWidth = 2 }: { name: IconName; size?: number; color: string; strokeWidth?: number }) {
  return (
    <Svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round">
      {name === "settings" && <Circle cx={12} cy={12} r={3} />}
      {(name === "x" || name === "alert") && <Circle cx={12} cy={12} r={9.5} />}
      <Path d={PATHS[name]} />
    </Svg>
  );
}

export function StopIcon({ size = 12, color }: { size?: number; color: string }) {
  return (
    <Svg width={size} height={size} viewBox="0 0 24 24">
      <Rect x={4} y={4} width={16} height={16} rx={4} fill={color} />
    </Svg>
  );
}
