import { Text, View } from "react-native";
import Svg, { Circle } from "react-native-svg";
import type { RateWindow } from "./api";
import { c } from "./theme";

// Compact 5-hour limit ring, like the desktop sidebar footer: fills with use, turns red past 80%.
export function UsageRing({ window, size = 28 }: { window: RateWindow; size?: number }) {
  if (!window) return null;
  const used = Math.max(0, Math.min(100, window.usedPercent));
  const stroke = 3;
  const r = (size - stroke) / 2;
  const len = 2 * Math.PI * r;
  const color = used > 80 ? c.danger : c.accent;
  const resets = window.resetsAt ? Math.max(0, Math.round((window.resetsAt * 1000 - Date.now()) / 60000)) : null;
  return (
    <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }} accessibilityLabel={`5-hour limit ${Math.round(used)}% used`}>
      <View style={{ width: size, height: size }}>
        <Svg width={size} height={size} style={{ transform: [{ rotate: "-90deg" }] }}>
          <Circle cx={size / 2} cy={size / 2} r={r} stroke={c.raised} strokeWidth={stroke} fill="none" />
          {used > 0 && <Circle cx={size / 2} cy={size / 2} r={r} stroke={color} strokeWidth={stroke} fill="none" strokeLinecap="round" strokeDasharray={`${(used / 100) * len} ${len}`} />}
        </Svg>
      </View>
      <View>
        <Text style={{ color: c.text, fontSize: 12.5, fontWeight: "700", fontVariant: ["tabular-nums"] }}>{Math.round(used)}%</Text>
        <Text style={{ color: c.faint, fontSize: 10.5 }}>{resets == null ? "5h limit" : resets >= 60 ? `resets ${Math.round(resets / 60)}h` : `resets ${resets}m`}</Text>
      </View>
    </View>
  );
}
