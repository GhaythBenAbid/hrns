import { useCallback, useEffect, useState } from "react";
import { ActivityIndicator, Alert, FlatList, Pressable, RefreshControl, Text, View } from "react-native";
import { ago, type Client, type TermInfo } from "../api";
import { Icon } from "../Icon";
import { ProjectFinder } from "../ProjectFinder";
import { ProjectIcon } from "../ProjectIcon";
import { alpha, c, sheet } from "../theme";

// Every shell running on the Mac, including ones opened in the desktop panel.
export function Terminals({ client, refreshKey, onOpen }: { client: Client; refreshKey: number; onOpen: (id: string) => void }) {
  const [list, setList] = useState<TermInfo[] | null>(null);
  const [recent, setRecent] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [picker, setPicker] = useState(false);
  const [busy, setBusy] = useState(false);
  const [refreshing, setRefreshing] = useState(false);

  const load = useCallback(async () => {
    try {
      setList(await client.terminals());
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, [client]);

  useEffect(() => {
    load();
  }, [load, refreshKey]);

  // Live list: sessions opened or closed on the Mac show up at once.
  useEffect(() => client.on((e) => e.type === "terminals" && setList(e.payload)), [client]);

  useEffect(() => {
    client.threads().then((t) => setRecent([...new Set(t.map((x) => x.cwd).filter((x): x is string => !!x))])).catch(() => {});
  }, [client]);

  async function start(cwd: string | null) {
    setPicker(false);
    setBusy(true);
    try {
      const t = await client.openTerminal(cwd);
      onOpen(t.id);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  function close(t: TermInfo) {
    Alert.alert(`Close ${t.title}?`, t.alive ? "The shell and anything running in it will stop." : undefined, [
      { text: "Cancel", style: "cancel" },
      { text: "Close", style: "destructive", onPress: () => client.closeTerminal(t.id).then(load).catch((e) => setError(e.message)) },
    ]);
  }

  const sorted = [...(list ?? [])].sort((a, b) => b.createdAt - a.createdAt);

  return (
    <View style={{ flex: 1 }}>
      <FlatList
        data={sorted}
        keyExtractor={(t) => t.id}
        contentContainerStyle={{ paddingBottom: 110 }}
        refreshControl={<RefreshControl refreshing={refreshing} tintColor={c.muted} onRefresh={async () => { setRefreshing(true); await load(); setRefreshing(false); }} />}
        ListHeaderComponent={
          <View style={styles.header}>
            <Text style={styles.h1}>Terminal</Text>
            <Text style={styles.lede}>Shells on your Mac. The ones open in hrns on the desktop are here too.</Text>
            {error && <Text style={styles.error}>{error}</Text>}
            {list === null && !error && <ActivityIndicator color={c.accent} style={{ marginTop: 30 }} />}
          </View>
        }
        ListEmptyComponent={
          list ? (
            <View style={styles.empty}>
              <View style={styles.emptyIcon}><Icon name="terminal" size={26} color={c.accent} /></View>
              <Text style={styles.emptyTitle}>No terminals yet</Text>
              <Text style={styles.emptyText}>Open one in any project. It keeps running when you leave, and you can pick it up on your Mac.</Text>
            </View>
          ) : undefined
        }
        renderItem={({ item: t }) => (
          <Pressable style={({ pressed }) => [styles.card, pressed && { backgroundColor: c.hover }]} onPress={() => onOpen(t.id)} onLongPress={() => close(t)}>
            <ProjectIcon client={client} path={t.cwd} size={34} />
            <View style={{ flex: 1, minWidth: 0 }}>
              <View style={styles.row1}>
                <Text style={styles.title} numberOfLines={1}>{t.title}</Text>
                <View style={[styles.state, { backgroundColor: alpha(t.alive ? c.green : c.faint, 0.14) }]}>
                  <View style={[styles.dot, { backgroundColor: t.alive ? c.green : c.faint }]} />
                  <Text style={[styles.stateText, { color: t.alive ? c.green : c.faint }]}>{t.alive ? "running" : "exited"}</Text>
                </View>
              </View>
              <Text style={styles.path} numberOfLines={1}>{t.cwd.replace(/^\/Users\/[^/]+/, "~")} · {ago(t.createdAt)}</Text>
            </View>
            <Pressable hitSlop={10} onPress={() => close(t)} style={styles.close} accessibilityLabel={`Close ${t.title}`}>
              <Icon name="x" size={18} color={c.faint} />
            </Pressable>
          </Pressable>
        )}
      />

      <Pressable style={({ pressed }) => [styles.fab, pressed && { opacity: 0.85 }]} onPress={() => setPicker(true)} disabled={busy}>
        {busy ? <ActivityIndicator size="small" color={c.bg} /> : <Icon name="plus" size={17} color={c.bg} strokeWidth={2.6} />}
        <Text style={styles.fabText}>New terminal</Text>
      </Pressable>

      <ProjectFinder client={client} visible={picker} recent={recent} onClose={() => setPicker(false)} onPick={start} />
    </View>
  );
}

const styles = sheet(() => ({
  header: { paddingHorizontal: 20, paddingTop: 10, paddingBottom: 14 },
  h1: { color: c.text, fontSize: 32, fontWeight: "700", letterSpacing: -0.8 },
  lede: { color: c.muted, fontSize: 14, lineHeight: 20, marginTop: 6 },
  error: { color: c.danger, marginTop: 10, fontSize: 13.5 },
  card: { flexDirection: "row", alignItems: "center", gap: 12, marginHorizontal: 14, marginBottom: 8, padding: 14, borderRadius: 18, borderWidth: 1, borderColor: c.border, backgroundColor: c.panel },
  row1: { flexDirection: "row", alignItems: "center", gap: 8 },
  title: { flexShrink: 1, color: c.text, fontSize: 16, fontWeight: "650" as "600" },
  state: { flexDirection: "row", alignItems: "center", gap: 5, height: 20, paddingHorizontal: 7, borderRadius: 10 },
  dot: { width: 6, height: 6, borderRadius: 3 },
  stateText: { fontSize: 11, fontWeight: "700" },
  path: { color: c.faint, fontSize: 12.5, marginTop: 3, fontFamily: c.mono },
  close: { width: 32, height: 32, alignItems: "center", justifyContent: "center" },
  empty: { alignItems: "center", paddingHorizontal: 40, marginTop: 50 },
  emptyIcon: { width: 60, height: 60, borderRadius: 20, alignItems: "center", justifyContent: "center", backgroundColor: alpha(c.accent, 0.12), marginBottom: 16 },
  emptyTitle: { color: c.text, fontSize: 18, fontWeight: "700" },
  emptyText: { color: c.muted, fontSize: 14, lineHeight: 20, textAlign: "center", marginTop: 6 },
  fab: { position: "absolute", right: 18, bottom: 18, backgroundColor: c.text, paddingHorizontal: 18, height: 48, borderRadius: 24, flexDirection: "row", alignItems: "center", gap: 8, shadowColor: "#000", shadowOpacity: 0.5, shadowRadius: 16, shadowOffset: { width: 0, height: 6 }, elevation: 8 },
  fabText: { color: c.bg, fontSize: 15, fontWeight: "700" },
}));
