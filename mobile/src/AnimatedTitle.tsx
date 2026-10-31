import { useEffect, useRef, useState } from "react";
import { AccessibilityInfo, Animated, Text, type StyleProp, type TextStyle } from "react-native";
import { c } from "./theme";

// Same effect as the desktop: when a chat is renamed (e.g. by the title model), the old title
// scrambles into the new one, settling left to right in the accent color, then glows briefly.
// A different chat (`id` changes) just swaps the text.

const GLYPHS = "abcdefghijklmnopqrstuvwxyz0123456789#%&*+<>/=";
const DURATION = 650;

let reduceMotion = false;
AccessibilityInfo.isReduceMotionEnabled().then((v) => (reduceMotion = v)).catch(() => {});

export function AnimatedTitle({ text, id, style, numberOfLines = 1 }: { text: string; id?: string; style?: StyleProp<TextStyle>; numberOfLines?: number }) {
  const [settled, setSettled] = useState(text);
  const [scrambled, setScrambled] = useState("");
  const glow = useRef(new Animated.Value(0)).current;
  const last = useRef({ text, id });
  const frame = useRef(0);

  useEffect(() => {
    const renamed = last.current.id === id && last.current.text !== text;
    last.current = { text, id };
    cancelAnimationFrame(frame.current);
    if (!renamed || reduceMotion) {
      setSettled(text);
      setScrambled("");
      return;
    }
    const from = settled + scrambled;
    const start = Date.now();
    const tick = () => {
      const p = Math.min(1, (Date.now() - start) / DURATION);
      const done = Math.floor(p * text.length);
      const len = Math.round(from.length + (text.length - from.length) * Math.min(1, p * 1.6));
      let rest = "";
      for (let i = done; i < Math.max(len, done); i++) rest += (text[i] ?? " ") === " " ? " " : GLYPHS[(Math.random() * GLYPHS.length) | 0];
      setSettled(text.slice(0, done));
      setScrambled(p < 1 ? rest : "");
      if (p < 1) frame.current = requestAnimationFrame(tick);
      else {
        setSettled(text);
        glow.setValue(1);
        Animated.timing(glow, { toValue: 0, duration: 900, useNativeDriver: false }).start();
      }
    };
    frame.current = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame.current);
  }, [text, id]); // eslint-disable-line react-hooks/exhaustive-deps

  const base = (Array.isArray(style) ? Object.assign({}, ...style) : style) as TextStyle | undefined;
  const color = glow.interpolate({ inputRange: [0, 1], outputRange: [String(base?.color ?? c.text), c.accent] });

  return (
    <Animated.Text style={[style, { color }]} numberOfLines={numberOfLines}>
      {settled}
      {scrambled ? <Text style={{ color: c.accent, opacity: 0.75, fontFamily: c.mono }}>{scrambled}</Text> : null}
    </Animated.Text>
  );
}
