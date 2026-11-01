import { useEffect, useState } from "react";
import { Image, Text, View } from "react-native";
import Svg, { Path, SvgXml } from "react-native-svg";
import type { Client, ProjectIcon as Icon } from "./api";
import { LOGOS } from "./logos";
import { alpha, c } from "./theme";

// Same rules as the desktop ProjectIcon: the project's own favicon/logo, else its framework or
// main language logo, else a two-letter monogram colored from the name.

const cache = new Map<string, Promise<Icon>>();
function load(client: Client, path: string) {
  let p = cache.get(path);
  if (!p) {
    // A failed lookup (Mac offline) isn't remembered, so the icon appears once it's back.
    p = client.projectIcon(path).catch(() => {
      cache.delete(path);
      return { image: null, tech: null };
    });
    cache.set(path, p);
  }
  return p;
}

/** Logos in near-black or near-white would vanish on one theme; draw those in the text color. */
function ink(hex: string) {
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return l < 0.12 || l > 0.9 ? c.text : `#${hex}`;
}

const HUES = [262, 217, 174, 142, 38, 24, 350, 322, 199, 84];
function monogram(name: string) {
  const words = name.match(/[\p{L}\p{N}]+/gu) ?? [];
  const first = [...(words[0] ?? "?")];
  const second = words.length > 1 ? [...words[words.length - 1]][0] : (first[1] ?? "");
  let h = 0;
  for (const ch of name.toLowerCase()) h = (h * 31 + ch.codePointAt(0)!) % HUES.length;
  return { text: (first[0] + second).toUpperCase(), hue: HUES[h] };
}

function decodeSvg(dataUrl: string): string | null {
  try {
    const b64 = dataUrl.slice(dataUrl.indexOf(",") + 1);
    const bin = atob(b64);
    // UTF-8 bytes → string without TextDecoder (not in every JS engine RN ships).
    return decodeURIComponent(Array.from(bin, (ch) => "%" + ch.charCodeAt(0).toString(16).padStart(2, "0")).join(""));
  } catch {
    return null;
  }
}

export function ProjectIcon({ client, path, size = 16 }: { client: Client; path: string | null | undefined; size?: number }) {
  const [icon, setIcon] = useState<Icon | null>(null);
  const [broken, setBroken] = useState(false);
  const name = path?.split("/").filter(Boolean).pop() ?? "";

  useEffect(() => {
    setIcon(null);
    setBroken(false);
    if (!path) return;
    let live = true;
    load(client, path).then((i) => live && setIcon(i));
    return () => {
      live = false;
    };
  }, [client, path]);

  const radius = size * 0.26;
  const box = { width: size, height: size, borderRadius: radius, alignItems: "center", justifyContent: "center", overflow: "hidden" } as const;
  const logo = icon?.tech ? LOGOS[icon.tech] : undefined;

  if (icon?.image && !broken) {
    if (icon.image.startsWith("data:image/svg")) {
      const xml = decodeSvg(icon.image);
      if (xml) return <View style={box}><SvgXml xml={xml} width={size} height={size} onError={() => setBroken(true)} /></View>;
    } else {
      return <Image source={{ uri: icon.image }} style={{ width: size, height: size, borderRadius: radius * 0.8 }} resizeMode="contain" onError={() => setBroken(true)} />;
    }
  }
  if (logo) {
    const color = ink(logo.hex);
    return (
      <View style={[box, { backgroundColor: alpha(color.startsWith("#") && color.length === 7 ? color : c.muted, 0.16) }]}>
        <Svg width={size * 0.64} height={size * 0.64} viewBox="0 0 24 24"><Path d={logo.path} fill={color} /></Svg>
      </View>
    );
  }
  if (icon && name) {
    const m = monogram(name);
    const light = c.scheme === "light";
    return (
      <View style={[box, { backgroundColor: `hsla(${m.hue}, 45%, 50%, 0.18)` }]}>
        <Text style={{ fontSize: size * 0.42, fontWeight: "700", letterSpacing: -0.3, color: `hsl(${m.hue}, ${light ? "60%, 38%" : "70%, 68%"})` }}>{m.text}</Text>
      </View>
    );
  }
  return <View style={[box, { backgroundColor: c.raised }]} />;
}
