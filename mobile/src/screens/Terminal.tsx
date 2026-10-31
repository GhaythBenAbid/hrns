import * as Clipboard from "expo-clipboard";
import * as SecureStore from "expo-secure-store";
import { useEffect, useRef, useState } from "react";
import { ActivityIndicator, Alert, Keyboard, KeyboardAvoidingView, Platform, Pressable, ScrollView, Text, TextInput, View, type TextInputInstance } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { WebView as RNWebView, type WebViewMessageEvent } from "react-native-webview";
import type { Client, TermInfo } from "../api";
import { Icon } from "../Icon";
import { alpha, c, sheet } from "../theme";

// A shell on the Mac, built for a phone:
// - Cards: each command is a card (command, colored output, status, time, copy / run again), drawn by
//   the Mac's terminal page from shell markers. Raw: the real terminal, for full-screen programs.
// - Typing happens in a native input bar (autocorrect off, history chips), sent on return. While a
//   full-screen program runs (vim, htop, claude…) every keystroke goes straight through instead.
// - A key row gives Esc, Tab, Ctrl, arrows and the usual control keys without a hardware keyboard.

const HISTORY = "hrns.termHistory";
const SIZES = [10, 11, 12, 13, 14, 16];

type Key = { label: string; send?: string; ctrl?: true; wide?: boolean };
const KEYS: Key[] = [
  { label: "esc", send: "\x1b" },
  { label: "tab", send: "\t" },
  { label: "ctrl", ctrl: true },
  { label: "↑", send: "\x1b[A" },
  { label: "↓", send: "\x1b[B" },
  { label: "←", send: "\x1b[D" },
  { label: "→", send: "\x1b[C" },
  { label: "^C", send: "\x03" },
  { label: "^D", send: "\x04" },
  { label: "^L", send: "\x0c" },
  { label: "|", send: "|" },
  { label: "~", send: "~" },
  { label: "/", send: "/" },
  { label: "-", send: "-" },
];

let fontSize = 12;

// react-native-webview's prop types don't line up with React Native 0.88's yet (they collapse to `never`);
// the component itself works, so type it by what we use.
type WebHandle = { injectJavaScript: (js: string) => void };
const WebView = RNWebView as unknown as React.ComponentType<Record<string, unknown> & { ref?: React.Ref<WebHandle> }>;

export function Terminal({ client, id, onBack, onReplace }: { client: Client; id: string; onBack: () => void; onReplace: (id: string) => void }) {
  const web = useRef<WebHandle>(null);
  const input = useRef<TextInputInstance>(null);
  const insets = useSafeAreaInsets();
  const [info, setInfo] = useState<TermInfo | null>(null);
  const [text, setText] = useState("");
  const [ctrl, setCtrl] = useState(false);
  const [state, setState] = useState<"loading" | "connected" | "offline" | "exited" | "gone">("loading");
  const [title, setTitle] = useState<string | null>(null);
  const [size, setSize] = useState(fontSize);
  const [history, setHistory] = useState<string[]>([]);
  const [keyboard, setKeyboard] = useState(false);
  const [mode, setMode] = useState<"blocks" | "raw">("blocks");
  const [cards, setCards] = useState(true);
  const [fullscreen, setFullscreen] = useState(false);
  const [cwd, setCwd] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const live = fullscreen;

  useEffect(() => {
    client.terminals().then((l) => setInfo(l.find((t) => t.id === id) ?? null)).catch(() => {});
    SecureStore.getItemAsync(HISTORY).then((h) => h && setHistory(JSON.parse(h))).catch(() => {});
    const show = Keyboard.addListener("keyboardDidShow", () => setKeyboard(true));
    const hide = Keyboard.addListener("keyboardDidHide", () => setKeyboard(false));
    return () => {
      show.remove();
      hide.remove();
    };
  }, [client, id]);

  // The page URL is fixed for the session; font size changes go through the page instead of reloading it.
  const [page] = useState(() => client.terminalPage(id, { scheme: c.scheme, bg: c.codeBg, fg: c.text, accent: c.accent, size }));

  const call = (js: string) => web.current?.injectJavaScript(`window.hrns && ${js}; true;`);
  const send = (data: string) => call(`window.hrns.input(${JSON.stringify(data)})`);

  function onMessage(e: WebViewMessageEvent) {
    const m = JSON.parse(e.nativeEvent.data) as { type: string; title?: string; mode?: "blocks" | "raw"; on?: boolean; command?: string; text?: string; path?: string; integrated?: boolean };
    if (m.type === "connected") setState((s) => (s === "exited" ? s : "connected"));
    else if (m.type === "disconnected") setState((s) => (s === "exited" || s === "gone" ? s : "offline"));
    else if (m.type === "exit") setState("exited");
    else if (m.type === "gone") setState("gone");
    else if (m.type === "title" && m.title) setTitle(m.title);
    else if (m.type === "tap") input.current?.focus();
    else if (m.type === "mode" && m.mode) setMode(m.mode);
    else if (m.type === "ready") setCards(!!m.integrated);
    else if (m.type === "fullscreen") setFullscreen(!!m.on);
    else if (m.type === "cwd" && m.path) setCwd(m.path);
    else if (m.type === "fill" && m.command != null) {
      setText(m.command);
      input.current?.focus();
    } else if (m.type === "copy" && m.text != null) {
      Clipboard.setStringAsync(m.text).then(() => {
        setToast("Copied");
        setTimeout(() => setToast(null), 1200);
      });
    }
  }

  function submit() {
    const cmd = text;
    send(cmd + "\r");
    setText("");
    call("window.hrns.bottom()");
    if (cmd.trim()) {
      const next = [cmd, ...history.filter((h) => h !== cmd)].slice(0, 30);
      setHistory(next);
      SecureStore.setItemAsync(HISTORY, JSON.stringify(next)).catch(() => {});
    }
  }

  function press(k: Key) {
    if (k.ctrl) return setCtrl(!ctrl);
    // Typed text goes first so "ls -la" + tab completes what's on screen.
    if (text) {
      send(text);
      setText("");
    }
    send(k.send!);
  }

  // With Ctrl armed, the next letter typed becomes its control character (Ctrl+R, Ctrl+A…).
  // In a full-screen program, each keystroke is sent as typed and the field stays empty.
  function change(next: string) {
    if (live && !ctrl) {
      if (next) send(next);
      return;
    }
    if (ctrl && next.length === text.length + 1) {
      const ch = next.slice(-1).toLowerCase();
      const code = ch.charCodeAt(0);
      if (code >= 97 && code <= 122) {
        send(String.fromCharCode(code - 96));
        setCtrl(false);
        return;
      }
    }
    setText(next);
  }

  function zoom(d: number) {
    const i = Math.max(0, Math.min(SIZES.length - 1, SIZES.indexOf(size) + d));
    fontSize = SIZES[i];
    setSize(SIZES[i]);
    call(`window.hrns.font(${SIZES[i]})`);
  }

  function kill() {
    Alert.alert("Close this terminal?", "The shell stops on your Mac too.", [
      { text: "Cancel", style: "cancel" },
      { text: "Close", style: "destructive", onPress: () => client.closeTerminal(id).finally(onBack) },
    ]);
  }

  const dead = state === "exited" || state === "gone";
  const [restarting, setRestarting] = useState(false);
  async function restart() {
    setRestarting(true);
    try {
      if (state === "exited") await client.closeTerminal(id).catch(() => {});
      const t = await client.openTerminal(info?.cwd ?? null);
      onReplace(t.id);
    } catch {
      setRestarting(false);
    }
  }
  const chips = history.filter((h) => !text || h.startsWith(text)).slice(0, 8);

  return (
    <KeyboardAvoidingView style={{ flex: 1, backgroundColor: c.codeBg }} behavior={Platform.OS === "ios" ? "padding" : undefined}>
      <View style={styles.header}>
        <Pressable onPress={onBack} hitSlop={10} style={styles.iconBtn} accessibilityLabel="Back">
          <Icon name="back" size={24} color={c.text} strokeWidth={2.2} />
        </Pressable>
        <View style={{ flex: 1, minWidth: 0 }}>
          <Text style={styles.title} numberOfLines={1}>{info?.title ?? "Terminal"}</Text>
          <View style={styles.subRow}>
            <View style={[styles.dot, { backgroundColor: state === "connected" ? c.green : dead ? c.faint : c.amber }]} />
            <Text style={styles.sub} numberOfLines={1}>
              {state === "connected" ? (fullscreen && title) || (cwd ?? info?.cwd ?? "").replace(/^\/Users\/[^/]+/, "~") : state === "loading" ? "Connecting…" : state === "offline" ? "Reconnecting…" : state === "exited" ? "Process exited" : "Closed on your Mac"}
            </Text>
          </View>
        </View>
        {cards && !fullscreen && (
          <View style={styles.seg}>
            {(["blocks", "raw"] as const).map((m) => (
              <Pressable key={m} onPress={() => call(`window.hrns.mode("${m}")`)} style={[styles.segBtn, mode === m && styles.segOn]} accessibilityState={{ selected: mode === m }}>
                <Text style={[styles.segText, mode === m && { color: c.text }]}>{m === "blocks" ? "Cards" : "Raw"}</Text>
              </Pressable>
            ))}
          </View>
        )}
        <Pressable onPress={() => zoom(-1)} hitSlop={6} style={styles.iconBtn} accessibilityLabel="Smaller text"><Text style={[styles.aa, { fontSize: 13 }]}>A</Text></Pressable>
        <Pressable onPress={() => zoom(1)} hitSlop={6} style={styles.iconBtn} accessibilityLabel="Larger text"><Text style={[styles.aa, { fontSize: 18 }]}>A</Text></Pressable>
        <Pressable onPress={kill} hitSlop={6} style={styles.iconBtn} accessibilityLabel="Close terminal"><Icon name="trash" size={18} color={c.muted} /></Pressable>
      </View>

      <View style={{ flex: 1 }}>
        <WebView
          ref={web}
          source={{ uri: page }}
          onMessage={onMessage}
          originWhitelist={["*"]}
          style={{ flex: 1, backgroundColor: c.codeBg }}
          containerStyle={{ backgroundColor: c.codeBg }}
          bounces={false}
          overScrollMode="never"
          setSupportMultipleWindows={false}
          hideKeyboardAccessoryView
          keyboardDisplayRequiresUserAction
          javaScriptEnabled
          onError={() => setState("offline")}
        />
        {toast && (
          <View style={styles.toast} pointerEvents="none"><Text style={styles.toastText}>{toast}</Text></View>
        )}
        {state === "loading" && (
          <View style={styles.loading} pointerEvents="none">
            <ActivityIndicator color={c.accent} />
          </View>
        )}
      </View>

      <View style={[styles.dock, { paddingBottom: keyboard ? 6 : Math.max(insets.bottom, 8) }]}>
        {dead && (
          <Pressable style={({ pressed }) => [styles.restart, pressed && { opacity: 0.85 }]} onPress={restart} disabled={restarting}>
            {restarting ? <ActivityIndicator size="small" color={c.accentInk} /> : <Icon name="terminal" size={17} color={c.accentInk} />}
            <Text style={styles.restartText}>New shell in {info?.title ?? "this folder"}</Text>
          </Pressable>
        )}
        {fullscreen && !dead && (
          <Text style={styles.liveNote}>Full-screen program: keys go straight through. Use esc / ^C to leave.</Text>
        )}
        {chips.length > 0 && !dead && !live && (
          <ScrollView horizontal showsHorizontalScrollIndicator={false} keyboardShouldPersistTaps="always" contentContainerStyle={styles.chips}>
            {chips.map((h) => (
              <Pressable key={h} style={({ pressed }) => [styles.chip, pressed && { backgroundColor: c.hover }]} onPress={() => setText(h)} onLongPress={() => { setText(""); send(h + "\r"); }}>
                <Text style={styles.chipText} numberOfLines={1}>{h}</Text>
              </Pressable>
            ))}
          </ScrollView>
        )}

        <ScrollView horizontal showsHorizontalScrollIndicator={false} keyboardShouldPersistTaps="always" contentContainerStyle={styles.keys}>
          {KEYS.map((k) => {
            const on = k.ctrl && ctrl;
            return (
              <Pressable key={k.label} disabled={dead} onPress={() => press(k)} style={({ pressed }) => [styles.key, on && styles.keyOn, pressed && { backgroundColor: c.hover }, dead && { opacity: 0.4 }]}>
                <Text style={[styles.keyText, on && { color: c.accentInk }]}>{k.label}</Text>
              </Pressable>
            );
          })}
          {keyboard && (
            <Pressable onPress={() => Keyboard.dismiss()} style={({ pressed }) => [styles.key, pressed && { backgroundColor: c.hover }]} accessibilityLabel="Hide keyboard">
              <Icon name="keyboardDown" size={17} color={c.muted} />
            </Pressable>
          )}
        </ScrollView>

        <View style={[styles.bar, ctrl && { borderColor: c.accent }]}>
          <Text style={styles.prompt}>{ctrl ? "^" : "$"}</Text>
          <TextInput
            ref={input}
            style={styles.input}
            value={text}
            onChangeText={change}
            onSubmitEditing={() => (live ? send("\r") : submit())}
            onKeyPress={(e) => live && e.nativeEvent.key === "Backspace" && send("\x7f")}
            blurOnSubmit={false}
            editable={!dead}
            placeholder={dead ? "This shell has ended" : ctrl ? "Ctrl + a letter…" : live ? "Typing goes straight to the program" : "Type a command"}
            placeholderTextColor={c.faint}
            autoCapitalize="none"
            autoCorrect={false}
            autoComplete="off"
            spellCheck={false}
            keyboardType={Platform.OS === "android" ? "visible-password" : "ascii-capable"}
            returnKeyType="send"
            smartInsertDelete={false}
          />
          <Pressable disabled={dead} onPress={submit} style={({ pressed }) => [styles.send, pressed && { opacity: 0.8 }, dead && { opacity: 0.3 }]} accessibilityLabel={text ? "Run" : "Press return"}>
            <Icon name="send" size={17} color={c.accentInk} strokeWidth={2.6} />
          </Pressable>
        </View>
      </View>
    </KeyboardAvoidingView>
  );
}

const styles = sheet(() => ({
  header: { flexDirection: "row", alignItems: "center", gap: 4, paddingHorizontal: 10, paddingVertical: 8, borderBottomWidth: 1, borderBottomColor: c.border, backgroundColor: c.bg },
  iconBtn: { width: 36, height: 36, alignItems: "center", justifyContent: "center", borderRadius: 10 },
  title: { color: c.text, fontSize: 16, fontWeight: "700" },
  subRow: { flexDirection: "row", alignItems: "center", gap: 6, marginTop: 2 },
  dot: { width: 6, height: 6, borderRadius: 3 },
  sub: { flexShrink: 1, color: c.faint, fontSize: 12, fontFamily: c.mono },
  aa: { color: c.muted, fontWeight: "700" },
  loading: { position: "absolute", inset: 0, alignItems: "center", justifyContent: "center" },
  dock: { borderTopWidth: 1, borderTopColor: c.border, backgroundColor: c.bg, paddingTop: 8, gap: 8 },
  chips: { gap: 6, paddingHorizontal: 10 },
  chip: { height: 28, maxWidth: 220, paddingHorizontal: 10, borderRadius: 8, justifyContent: "center", backgroundColor: c.raised },
  chipText: { color: c.muted, fontSize: 12.5, fontFamily: c.mono },
  keys: { gap: 6, paddingHorizontal: 10 },
  key: { minWidth: 42, height: 36, paddingHorizontal: 10, borderRadius: 10, alignItems: "center", justifyContent: "center", backgroundColor: c.panel, borderWidth: 1, borderColor: c.border },
  keyOn: { backgroundColor: c.accent, borderColor: c.accent },
  keyText: { color: c.text, fontSize: 14, fontWeight: "600", fontFamily: c.mono },
  bar: { flexDirection: "row", alignItems: "center", gap: 8, marginHorizontal: 10, paddingLeft: 14, paddingRight: 5, height: 48, borderRadius: 24, borderWidth: 1, borderColor: c.border, backgroundColor: alpha(c.text, 0.04) },
  prompt: { color: c.accent, fontFamily: c.mono, fontSize: 15, fontWeight: "700" },
  input: { flex: 1, color: c.text, fontFamily: c.mono, fontSize: 15, paddingVertical: 0 },
  seg: { flexDirection: "row", padding: 2, borderRadius: 9, backgroundColor: c.raised, marginRight: 2 },
  segBtn: { paddingHorizontal: 9, height: 26, borderRadius: 7, justifyContent: "center" },
  segOn: { backgroundColor: c.bg },
  segText: { color: c.faint, fontSize: 12, fontWeight: "700" },
  toast: { position: "absolute", top: 12, alignSelf: "center", paddingHorizontal: 14, height: 30, borderRadius: 15, justifyContent: "center", backgroundColor: c.text },
  toastText: { color: c.bg, fontSize: 13, fontWeight: "700" },
  liveNote: { color: c.accent, fontSize: 12, paddingHorizontal: 14 },
  restart: { flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 8, height: 44, marginHorizontal: 10, borderRadius: 14, backgroundColor: c.accent },
  restartText: { color: c.accentInk, fontSize: 15, fontWeight: "700" },
  send: { width: 38, height: 38, borderRadius: 19, alignItems: "center", justifyContent: "center", backgroundColor: c.accent },
}));
