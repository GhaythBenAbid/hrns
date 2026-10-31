import * as SecureStore from "expo-secure-store";
import { useCallback, useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Modal, Pressable, RefreshControl, SectionList, StyleSheet, Text, View } from "react-native";
import { ago, base, tokens, type Client, type Summary } from "../api";
import { Icon } from "../Icon";
import { enableNotifications, notificationsEnabled } from "../notify";
import { c } from "../theme";

type Props = {
  client: Client;
  host: string;
  connected: boolean;
  streaming: Set<string>;
  waiting: Set<string>;
  onOpen: (id: string) => void;
  onNew: (cwd: string) => void;
  refreshKey: number;
};

const DISMISSED = "hrns.notifyPromptDismissed";

export function Chats({ client, host, connected, streaming, waiting, onOpen, onNew, refreshKey }: Props) {
  const [threads, setThreads] = useState<Summary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [picker, setPicker] = useState(false);
  const [askNotify, setAskNotify] = useState(false);

  const load = useCallback(async () => {
    try {
      setThreads(await client.threads());
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, [client]);

  // Refetch when the parent sees a finished turn or a reconnect.
  useEffect(() => {
    load();
  }, [load, refreshKey]);

  useEffect(() => {
    Promise.all([notificationsEnabled(), SecureStore.getItemAsync(DISMISSED)]).then(([on, dismissed]) => setAskNotify(!on && !dismissed));
  }, []);

  const sections = useMemo(() => {
    const map = new Map<string, Summary[]>();
    for (const t of threads ?? []) {
      const k = t.cwd ?? "";
      if (!map.has(k)) map.set(k, []);
      map.get(k)!.push(t);
    }
    return [...map.entries()].map(([cwd, data]) => ({ cwd, title: base(cwd) || "No project", data }));
  }, [threads]);
  const projects = sections.map((s) => s.cwd).filter(Boolean);

  const header = (
    <View>
      <View style={styles.header}>
        <View style={styles.brandRow}>
          <View style={styles.mark}><Text style={styles.markText}>{">_"}</Text></View>
          <Text style={styles.brand}>hrns</Text>
        </View>
        <Text style={styles.h1}>Chats</Text>
        <View style={styles.pill}>
          <View style={[styles.dot, { backgroundColor: connected ? c.green : c.faint }]} />
          <Text style={styles.pillText}>{connected ? host : "Reconnecting…"}</Text>
          <Text style={styles.pillSub}>· Tailscale</Text>
        </View>
      </View>

      {askNotify && (
        <View style={styles.notice}>
          <View style={styles.noticeIcon}><Icon name="bell" size={18} color={c.accent} /></View>
          <View style={{ flex: 1 }}>
            <Text style={styles.noticeTitle}>Get notified when Codex needs you</Text>
            <Text style={styles.noticeText}>Approve commands and see finished work right from your notifications.</Text>
            <View style={styles.noticeActions}>
              <Pressable style={styles.btnPrimary} onPress={async () => { await enableNotifications(); setAskNotify(false); }}>
                <Text style={styles.btnPrimaryText}>Enable</Text>
              </Pressable>
              <Pressable style={styles.btnGhost} onPress={() => { SecureStore.setItemAsync(DISMISSED, "1"); setAskNotify(false); }}>
                <Text style={styles.btnGhostText}>Not now</Text>
              </Pressable>
            </View>
          </View>
        </View>
      )}
      {error && <Text style={styles.error}>{error}</Text>}
    </View>
  );

  return (
    <View style={styles.wrap}>
      {threads === null && !error ? (
        <>
          {header}
          <ActivityIndicator color={c.accent} style={{ marginTop: 40 }} />
        </>
      ) : (
        <SectionList
          sections={sections}
          keyExtractor={(t) => t.id}
          ListHeaderComponent={header}
          contentContainerStyle={{ paddingBottom: 110 }}
          stickySectionHeadersEnabled
          refreshControl={<RefreshControl refreshing={refreshing} tintColor={c.muted} onRefresh={async () => { setRefreshing(true); await load(); setRefreshing(false); }} />}
          renderSectionHeader={({ section }) => (
            <View style={styles.section}>
              <Icon name="folder" size={13} color={c.faint} />
              <Text style={styles.sectionText}>{section.title.toUpperCase()}</Text>
              <View style={styles.count}><Text style={styles.countText}>{section.data.length}</Text></View>
            </View>
          )}
          renderItem={({ item: t }) => {
            const run = streaming.has(t.id);
            const wait = waiting.has(t.id);
            const tok = Object.values(t.usage ?? {}).reduce((a, u) => a + u.input + u.output, 0);
            return (
              <Pressable style={({ pressed }) => [styles.card, wait && styles.cardWait, pressed && { backgroundColor: c.hover }]} onPress={() => onOpen(t.id)}>
                <View style={styles.status}>
                  {wait ? <Icon name="alert" size={15} color={c.amber} strokeWidth={2.2} />
                    : run ? <ActivityIndicator size="small" color={c.accent} />
                    : t.failed ? <Icon name="x" size={15} color={c.danger} strokeWidth={2.2} />
                    : <Icon name="chat" size={15} color={c.faint} />}
                </View>
                <View style={{ flex: 1, gap: 5 }}>
                  <View style={styles.row1}>
                    <Text style={styles.title} numberOfLines={1}>{t.title}</Text>
                    <Text style={styles.ago}>{ago(t.updatedAt)}</Text>
                  </View>
                  {wait ? (
                    <Text style={[styles.preview, { color: c.amber }]}>Waiting for your approval</Text>
                  ) : t.preview ? (
                    <Text style={styles.preview} numberOfLines={2}>{t.preview.replace(/[#*`_>]/g, "").replace(/\s+/g, " ")}</Text>
                  ) : null}
                  <View style={styles.chips}>
                    <View style={styles.chip}><Text style={[styles.chipText, { color: c.accent }]}>{t.model.replace(/^gpt-/i, "")}</Text></View>
                    {t.files > 0 && (
                      <View style={styles.chip}>
                        <Text style={[styles.chipText, { color: c.green }]}>+{t.additions}</Text>
                        <Text style={[styles.chipText, { color: c.danger }]}>−{t.deletions}</Text>
                      </View>
                    )}
                    {tok > 0 && (
                      <View style={styles.chip}>
                        <Icon name="bolt" size={10} color={c.faint} strokeWidth={2.4} />
                        <Text style={styles.chipText}>{tokens(tok)}</Text>
                      </View>
                    )}
                  </View>
                </View>
              </Pressable>
            );
          }}
          ListEmptyComponent={<Text style={styles.empty}>No chats yet. Start one from your Mac, or tap New chat.</Text>}
        />
      )}

      {projects.length > 0 && (
        <Pressable style={({ pressed }) => [styles.fab, pressed && { opacity: 0.85 }]} onPress={() => (projects.length === 1 ? onNew(projects[0]) : setPicker(true))}>
          <Icon name="plus" size={17} color={c.bg} strokeWidth={2.6} />
          <Text style={styles.fabText}>New chat</Text>
        </Pressable>
      )}

      <Modal visible={picker} transparent animationType="fade" onRequestClose={() => setPicker(false)}>
        <Pressable style={styles.backdrop} onPress={() => setPicker(false)}>
          <View style={styles.sheet}>
            <View style={styles.grabber} />
            <Text style={styles.sheetTitle}>Start a chat in</Text>
            {projects.map((p) => (
              <Pressable key={p} style={({ pressed }) => [styles.sheetRow, pressed && { backgroundColor: c.hover }]} onPress={() => { setPicker(false); onNew(p); }}>
                <View style={styles.sheetIcon}><Icon name="folder" size={16} color={c.muted} /></View>
                <View style={{ flex: 1 }}>
                  <Text style={styles.sheetName}>{base(p)}</Text>
                  <Text style={styles.sheetPath} numberOfLines={1}>{p.replace(/^\/Users\/[^/]+/, "~")}</Text>
                </View>
              </Pressable>
            ))}
          </View>
        </Pressable>
      </Modal>
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: { flex: 1 },
  header: { paddingHorizontal: 20, paddingTop: 10, paddingBottom: 8 },
  brandRow: { flexDirection: "row", alignItems: "center", gap: 8, marginBottom: 14 },
  mark: { width: 26, height: 26, borderRadius: 8, backgroundColor: c.raised, borderWidth: 1, borderColor: c.border, alignItems: "center", justifyContent: "center" },
  markText: { color: c.accent, fontFamily: c.mono, fontSize: 10.5, fontWeight: "800" },
  brand: { color: c.muted, fontSize: 14, fontWeight: "700", letterSpacing: 0.2 },
  h1: { color: c.text, fontSize: 32, fontWeight: "700", letterSpacing: -0.8 },
  pill: { flexDirection: "row", alignItems: "center", alignSelf: "flex-start", gap: 6, marginTop: 10, height: 28, paddingHorizontal: 11, borderRadius: 14, borderWidth: 1, borderColor: c.border, backgroundColor: c.panel },
  dot: { width: 7, height: 7, borderRadius: 4 },
  pillText: { color: c.text, fontSize: 12.5, fontWeight: "600" },
  pillSub: { color: c.faint, fontSize: 12.5 },
  notice: { flexDirection: "row", gap: 12, marginHorizontal: 16, marginTop: 14, padding: 14, borderRadius: 18, borderWidth: 1, borderColor: "#3a2f5e", backgroundColor: "#13111c" },
  noticeIcon: { width: 34, height: 34, borderRadius: 11, backgroundColor: "#211b35", alignItems: "center", justifyContent: "center" },
  noticeTitle: { color: c.text, fontSize: 15, fontWeight: "700" },
  noticeText: { color: c.muted, fontSize: 13, lineHeight: 18, marginTop: 3 },
  noticeActions: { flexDirection: "row", gap: 8, marginTop: 12 },
  btnPrimary: { height: 34, paddingHorizontal: 16, borderRadius: 10, backgroundColor: c.text, justifyContent: "center" },
  btnPrimaryText: { color: c.bg, fontSize: 13.5, fontWeight: "700" },
  btnGhost: { height: 34, paddingHorizontal: 14, borderRadius: 10, borderWidth: 1, borderColor: c.border, justifyContent: "center" },
  btnGhostText: { color: c.muted, fontSize: 13.5, fontWeight: "600" },
  error: { color: c.danger, marginHorizontal: 20, marginTop: 10, fontSize: 13.5 },
  section: { flexDirection: "row", alignItems: "center", gap: 7, paddingHorizontal: 22, paddingTop: 18, paddingBottom: 8, backgroundColor: c.bg },
  sectionText: { color: c.faint, fontSize: 11.5, fontWeight: "700", letterSpacing: 0.9 },
  count: { backgroundColor: c.raised, paddingHorizontal: 6, borderRadius: 7, minWidth: 18, alignItems: "center" },
  countText: { color: c.faint, fontSize: 10.5, fontWeight: "700", lineHeight: 15 },
  card: { flexDirection: "row", gap: 11, marginHorizontal: 14, marginBottom: 8, padding: 14, borderRadius: 18, borderWidth: 1, borderColor: c.border, backgroundColor: c.panel },
  cardWait: { borderColor: "#5a4516", backgroundColor: "#16130c" },
  status: { width: 18, paddingTop: 2, alignItems: "center" },
  row1: { flexDirection: "row", alignItems: "center", gap: 8 },
  title: { flex: 1, color: c.text, fontSize: 15.5, fontWeight: "650" as "600", letterSpacing: -0.1 },
  ago: { color: c.faint, fontSize: 12, fontVariant: ["tabular-nums"] },
  preview: { color: c.muted, fontSize: 13.5, lineHeight: 19 },
  chips: { flexDirection: "row", gap: 6, marginTop: 3 },
  chip: { flexDirection: "row", alignItems: "center", gap: 4, backgroundColor: c.raised, paddingHorizontal: 7, height: 21, borderRadius: 7 },
  chipText: { color: c.faint, fontSize: 11.5, fontWeight: "600" },
  empty: { color: c.faint, textAlign: "center", marginTop: 60, paddingHorizontal: 40, lineHeight: 20 },
  fab: { position: "absolute", right: 18, bottom: 18, backgroundColor: c.text, paddingHorizontal: 18, height: 48, borderRadius: 24, flexDirection: "row", alignItems: "center", gap: 8, shadowColor: "#000", shadowOpacity: 0.5, shadowRadius: 16, shadowOffset: { width: 0, height: 6 }, elevation: 8 },
  fabText: { color: c.bg, fontSize: 15, fontWeight: "700" },
  backdrop: { flex: 1, backgroundColor: "rgba(0,0,0,0.65)", justifyContent: "flex-end" },
  sheet: { backgroundColor: c.panel, borderTopLeftRadius: 26, borderTopRightRadius: 26, padding: 14, paddingBottom: 36, borderWidth: 1, borderColor: c.border },
  grabber: { alignSelf: "center", width: 38, height: 4, borderRadius: 2, backgroundColor: c.border, marginBottom: 14 },
  sheetTitle: { color: c.faint, fontSize: 12, fontWeight: "700", letterSpacing: 0.8, textTransform: "uppercase", marginBottom: 8, marginLeft: 8 },
  sheetRow: { flexDirection: "row", alignItems: "center", gap: 12, padding: 10, borderRadius: 14 },
  sheetIcon: { width: 36, height: 36, borderRadius: 11, backgroundColor: c.raised, alignItems: "center", justifyContent: "center" },
  sheetName: { color: c.text, fontSize: 15.5, fontWeight: "600" },
  sheetPath: { color: c.faint, fontSize: 12.5, marginTop: 2 },
});
