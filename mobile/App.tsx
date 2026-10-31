import * as SecureStore from "expo-secure-store";
import { StatusBar } from "expo-status-bar";
import { useEffect, useMemo, useRef, useState } from "react";
import { ActivityIndicator, AppState, BackHandler, Pressable, StyleSheet, Text, View } from "react-native";
import { SafeAreaProvider, SafeAreaView, useSafeAreaInsets } from "react-native-safe-area-context";
import { Client, type Prefs, type Server } from "./src/api";
import { Icon, type IconName } from "./src/Icon";
import { APPROVAL, notify, onNotificationResponse, setupNotifications } from "./src/notify";
import { loadPrefs } from "./src/prefs";
import { DEMO_SERVER, demoScene, installDemo } from "./src/demo";

if (demoScene) installDemo();
import { Chat } from "./src/screens/Chat";
import { Chats } from "./src/screens/Chats";
import { Pair } from "./src/screens/Pair";
import { Settings } from "./src/screens/Settings";
import { Usage } from "./src/screens/Usage";
import { c } from "./src/theme";

const KEY = "hrns.server";

type Tab = "chats" | "usage" | "settings";
type Route = { name: "list" } | { name: "chat"; id: string | null; cwd: string | null };

export default function App() {
  const [server, setServer] = useState<Server | null | undefined>(demoScene ? DEMO_SERVER : undefined);
  const [route, setRoute] = useState<Route>(demoScene?.startsWith("chat-") ? { name: "chat", id: demoScene.slice(5), cwd: null } : { name: "list" });
  const [connected, setConnected] = useState(false);
  const [host, setHost] = useState("your Mac");
  const [streaming, setStreaming] = useState<Set<string>>(new Set());
  const [waiting, setWaiting] = useState<Set<string>>(new Set());
  const [refreshKey, setRefreshKey] = useState(0);
  const [tab, setTab] = useState<Tab>(demoScene === "usage" || demoScene === "settings" ? demoScene : "chats");
  const [prefs, setPrefs] = useState<Prefs>({});
  const [accounts, setAccounts] = useState<{ id: string; email?: string | null }[]>([]);

  useEffect(() => {
    if (!demoScene) loadPrefs().then(setPrefs);
  }, []);

  useEffect(() => {
    if (demoScene) return;
    SecureStore.getItemAsync(KEY).then((s) => setServer(s ? JSON.parse(s) : null));
  }, []);

  const client = useMemo(() => (server ? new Client(server) : null), [server]);
  // Which chat is on screen: no notification for what you're already looking at.
  const openChat = useRef<string | null>(null);
  openChat.current = route.name === "chat" ? route.id : null;

  // Approve / Deny straight from the notification, or tap to open the chat.
  useEffect(() => {
    if (!client || demoScene) return;
    setupNotifications();
    const sub = onNotificationResponse((r) => {
      const data = r.notification.request.content.data as Record<string, string>;
      if (r.actionIdentifier === "approve" && data.key) client.approve(data.key, "accept").catch(() => {});
      else if (r.actionIdentifier === "deny" && data.key) client.approve(data.key, "decline").catch(() => {});
      else if (data.threadId) {
        setTab("chats");
        setRoute({ name: "chat", id: data.threadId, cwd: null });
      }
    });
    return () => sub.remove();
  }, [client]);

  // One live connection for the whole app: drives list badges and the open chat.
  useEffect(() => {
    if (!client) return;
    client.info().then((i) => {
      if (i.host) setHost(i.host);
      setAccounts(i.accounts);
    }).catch(() => {});
    const off = client.on((e) => {
      if (e.type === "connection") setConnected(e.connected);
      if (e.type === "resync") setRefreshKey((k) => k + 1);
      if (e.type === "notify") {
        notify(e.payload.title, e.payload.body, e.payload.threadId ? { threadId: e.payload.threadId } : {});
        return;
      }
      if (e.type !== "chat") return;
      const ev = e.payload;
      const elsewhere = openChat.current !== ev.threadId || AppState.currentState !== "active";
      if (elsewhere && ev.kind === "approval") {
        const a = ev.approval;
        const what = a.command ? `$ ${a.command}` : a.changes?.length ? `Edit ${a.changes.map((ch) => ch.path.split("/").pop()).join(", ")}` : a.reason ?? "Needs your permission";
        notify("Codex needs approval", what, { threadId: ev.threadId, key: a.key }, APPROVAL);
      }
      if (elsewhere && ev.kind === "done") {
        const m = ev.message;
        const files = m?.parts?.filter((p) => p.type === "fileChange").reduce((n, p) => n + (p.type === "fileChange" ? p.changes.length : 0), 0) ?? 0;
        const summary = m?.error && m.error !== "Stopped" ? `Failed: ${m.error}` : m?.text ? m.text.replace(/[#*`_>]/g, "").replace(/\s+/g, " ").slice(0, 140) : "Turn finished";
        notify(files ? `Codex finished · ${files} file${files > 1 ? "s" : ""} changed` : "Codex finished", summary, { threadId: ev.threadId });
      }
      const { kind, threadId } = e.payload;
      if (kind === "done") {
        setStreaming((s) => without(s, threadId));
        setWaiting((s) => without(s, threadId));
        setRefreshKey((k) => k + 1);
      } else {
        setStreaming((s) => (s.has(threadId) ? s : new Set(s).add(threadId)));
        if (kind === "approval") setWaiting((s) => new Set(s).add(threadId));
        if (kind === "approvalResolved") setWaiting((s) => without(s, threadId));
      }
    });
    client.connect();
    return () => {
      off();
      client.close();
    };
  }, [client]);

  useEffect(() => {
    const sub = BackHandler.addEventListener("hardwareBackPress", () => {
      if (route.name === "chat") {
        setRoute({ name: "list" });
        return true;
      }
      return false;
    });
    return () => sub.remove();
  }, [route]);

  async function pair(s: Server) {
    await SecureStore.setItemAsync(KEY, JSON.stringify(s));
    setServer(s);
  }

  async function unpair() {
    await SecureStore.deleteItemAsync(KEY);
    setServer(null);
    setRoute({ name: "list" });
    setTab("chats");
  }

  const accountLabel = (id: string) => accounts.find((a) => a.id === id)?.email ?? id;

  return (
    <SafeAreaProvider>
      <StatusBar style="light" />
      <SafeAreaView style={styles.root} edges={["top"]}>
        {server === undefined ? (
          <View style={styles.center}>
            <ActivityIndicator color={c.accent} />
          </View>
        ) : !server || !client ? (
          <Pair onPaired={pair} />
        ) : route.name === "chat" ? (
          <Chat key={route.id ?? "new"} client={client} threadId={route.id} cwd={route.cwd} prefs={prefs} onBack={() => setRoute({ name: "list" })} />
        ) : (
          <View style={{ flex: 1 }}>
            <View style={{ flex: 1 }}>
              {tab === "usage" ? (
                <Usage client={client} accountLabel={accountLabel} />
              ) : tab === "settings" ? (
                <Settings client={client} host={host} connected={connected} prefs={prefs} onPrefs={setPrefs} onUnpair={unpair} />
              ) : (
          <Chats
            client={client}
            host={host}
            connected={connected}
            streaming={streaming}
            waiting={waiting}
            refreshKey={refreshKey}
            onOpen={(id) => setRoute({ name: "chat", id, cwd: null })}
            onNew={(cwd) => setRoute({ name: "chat", id: null, cwd })}
          />
              )}
            </View>
            <TabBar tab={tab} onTab={setTab} waiting={waiting.size > 0} />
          </View>
        )}
      </SafeAreaView>
    </SafeAreaProvider>
  );
}

const TABS: { id: Tab; label: string; icon: IconName }[] = [
  { id: "chats", label: "Chats", icon: "chat" },
  { id: "usage", label: "Usage", icon: "usage" },
  { id: "settings", label: "Settings", icon: "settings" },
];

function TabBar({ tab, onTab, waiting }: { tab: Tab; onTab: (t: Tab) => void; waiting: boolean }) {
  const insets = useSafeAreaInsets();
  return (
    <View style={[styles.tabs, { paddingBottom: Math.max(insets.bottom, 8) }]}>
      {TABS.map((t) => (
        <Pressable key={t.id} style={styles.tab} onPress={() => onTab(t.id)}>
          <View style={[styles.tabBar, tab === t.id && { backgroundColor: c.accent }]} />
          <Icon name={t.icon} size={22} color={tab === t.id ? c.text : c.faint} strokeWidth={tab === t.id ? 2.2 : 1.9} />
          <Text style={[styles.tabLabel, tab === t.id && { color: c.text }]}>{t.label}</Text>
          {t.id === "chats" && waiting && <View style={styles.tabDot} />}
        </Pressable>
      ))}
    </View>
  );
}

function without(s: Set<string>, id: string) {
  if (!s.has(id)) return s;
  const n = new Set(s);
  n.delete(id);
  return n;
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: c.bg },
  center: { flex: 1, alignItems: "center", justifyContent: "center" },
  tabs: { flexDirection: "row", borderTopWidth: 1, borderTopColor: c.border, backgroundColor: "#0b0b0d" },
  tab: { flex: 1, alignItems: "center", gap: 3, paddingTop: 0 },
  tabBar: { width: 22, height: 2.5, borderRadius: 2, marginBottom: 8, backgroundColor: "transparent" },
  tabLabel: { color: c.faint, fontSize: 11.5, fontWeight: "600" },
  tabDot: { position: "absolute", top: 8, right: "34%", width: 8, height: 8, borderRadius: 4, backgroundColor: c.amber },
});
