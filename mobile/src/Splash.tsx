import { useEffect, useRef } from "react";
import { Animated, Easing, Text, View } from "react-native";
import Svg, { Path } from "react-native-svg";
import { alpha, c, sheet } from "./theme";

// Startup screen, like the desktop: the tile pops in, the prompt mark draws itself, the cursor
// blinks, the wordmark rises letter by letter and a light sweeps the progress track.

const AnimatedPath = Animated.createAnimatedComponent(Path);
const ease = Easing.bezier(0.32, 0.72, 0, 1);
const WORD = "hrns".split("");

export function Splash({ label = "Connecting to your Mac" }: { label?: string }) {
  const pop = useRef(new Animated.Value(0)).current;
  const draw = useRef(new Animated.Value(1)).current;
  const cursor = useRef(new Animated.Value(0)).current;
  const glow = useRef(new Animated.Value(0)).current;
  const sweep = useRef(new Animated.Value(0)).current;
  const status = useRef(new Animated.Value(0)).current;
  const letters = useRef(WORD.map(() => new Animated.Value(0))).current;

  useEffect(() => {
    const anims = [
      Animated.timing(pop, { toValue: 1, duration: 600, easing: ease, useNativeDriver: true }),
      Animated.timing(draw, { toValue: 0, duration: 500, delay: 250, easing: Easing.bezier(0.65, 0, 0.35, 1), useNativeDriver: false }),
      Animated.sequence([Animated.delay(700), Animated.loop(Animated.sequence([
        Animated.timing(cursor, { toValue: 1, duration: 0, useNativeDriver: true }), Animated.delay(525),
        Animated.timing(cursor, { toValue: 0, duration: 0, useNativeDriver: true }), Animated.delay(525),
      ]))]),
      Animated.loop(Animated.sequence([
        Animated.timing(glow, { toValue: 1, duration: 1400, easing: Easing.inOut(Easing.sin), useNativeDriver: true }),
        Animated.timing(glow, { toValue: 0, duration: 1400, easing: Easing.inOut(Easing.sin), useNativeDriver: true }),
      ])),
      Animated.stagger(70, letters.map((v) => Animated.timing(v, { toValue: 1, duration: 500, delay: 550, easing: ease, useNativeDriver: true }))),
      Animated.timing(status, { toValue: 1, duration: 500, delay: 900, useNativeDriver: true }),
      Animated.loop(Animated.timing(sweep, { toValue: 1, duration: 1300, easing: Easing.bezier(0.45, 0, 0.55, 1), useNativeDriver: true })),
    ];
    anims.forEach((a) => a.start());
    return () => anims.forEach((a) => a.stop());
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <View style={styles.wrap} accessibilityRole="progressbar" accessibilityLabel={label}>
      <View style={styles.stage}>
        <Animated.View style={[styles.glow, { opacity: glow.interpolate({ inputRange: [0, 1], outputRange: [0.35, 0.8] }), transform: [{ scale: glow.interpolate({ inputRange: [0, 1], outputRange: [0.92, 1.06] }) }] }]} />
        <Animated.View style={[styles.tile, { opacity: pop, transform: [{ scale: pop.interpolate({ inputRange: [0, 1], outputRange: [0.82, 1] }) }] }]}>
          <Svg width={34} height={34} viewBox="0 0 24 24" fill="none">
            <AnimatedPath d="M6 7l5 5-5 5" stroke={c.text} strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round" strokeDasharray="14" strokeDashoffset={draw.interpolate({ inputRange: [0, 1], outputRange: [0, 14] })} />
          </Svg>
          <Animated.View style={[styles.cursor, { opacity: cursor }]} />
        </Animated.View>
      </View>

      <View style={styles.word}>
        {WORD.map((ch, i) => (
          <Animated.Text key={i} style={[styles.letter, { opacity: letters[i], transform: [{ translateY: letters[i].interpolate({ inputRange: [0, 1], outputRange: [8, 0] }) }] }]}>{ch}</Animated.Text>
        ))}
      </View>

      <Animated.View style={[styles.status, { opacity: status }]}>
        <View style={styles.track}>
          <Animated.View style={[styles.bar, { transform: [{ translateX: sweep.interpolate({ inputRange: [0, 1], outputRange: [-48, 120] }) }] }]} />
        </View>
        <Text style={styles.label}>{label}</Text>
      </Animated.View>
    </View>
  );
}

const styles = sheet(() => ({
  wrap: { flex: 1, alignItems: "center", justifyContent: "center", gap: 22, backgroundColor: c.bg },
  stage: { width: 120, height: 120, alignItems: "center", justifyContent: "center" },
  glow: { position: "absolute", width: 110, height: 110, borderRadius: 55, backgroundColor: alpha(c.accent, 0.22), shadowColor: c.accent, shadowOpacity: 0.8, shadowRadius: 30, shadowOffset: { width: 0, height: 0 } },
  tile: { width: 76, height: 76, borderRadius: 24, alignItems: "center", justifyContent: "center", backgroundColor: c.raised, borderWidth: 1, borderColor: c.border, shadowColor: "#000", shadowOpacity: 0.35, shadowRadius: 20, shadowOffset: { width: 0, height: 14 }, elevation: 10 },
  cursor: { position: "absolute", left: 43, top: 49, width: 13, height: 3, borderRadius: 2, backgroundColor: c.accent },
  word: { flexDirection: "row" },
  letter: { color: c.text, fontSize: 22, fontWeight: "700", letterSpacing: -0.4 },
  status: { alignItems: "center", gap: 10 },
  track: { width: 120, height: 2, borderRadius: 2, overflow: "hidden", backgroundColor: c.raised },
  bar: { width: 48, height: 2, borderRadius: 2, backgroundColor: c.accent },
  label: { color: c.faint, fontSize: 12, letterSpacing: 0.2 },
}));
