import { useEffect, useMemo, useState } from "react";
import { ActivityIndicator, FlatList, KeyboardAvoidingView, Modal, Platform, Pressable, Text, TextInput, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import type { Client, Project } from "./api";
import { fuzzy } from "./fuzzy";
import { Icon } from "./Icon";
import { ProjectIcon } from "./ProjectIcon";
import { c, sheet } from "./theme";

const LIMIT = 120;

// Projects found on the Mac are scanned once per app session (the walk takes a moment).
let scanned: Promise<Project[]> | null = null;

/** Pick a folder for a new chat: folders with chats first, then every project found on the Mac, fzf-style search. */
export function ProjectFinder({ client, visible, recent, onPick, onClose }: { client: Client; visible: boolean; recent: string[]; onPick: (path: string) => void; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [found, setFound] = useState<Project[] | null>(null);
  const insets = useSafeAreaInsets();

  useEffect(() => {
    if (!visible) return;
    setQuery("");
    scanned ??= client.projects().catch(() => {
      scanned = null;
      return [];
    });
    scanned.then(setFound);
  }, [visible, client]);

  const candidates = useMemo(() => {
    const seen = new Set<string>();
    const out: { path: string; recent: boolean }[] = [];
    for (const p of recent) if (p && !seen.has(p)) { seen.add(p); out.push({ path: p, recent: true }); }
    for (const p of [...(found ?? [])].sort((a, b) => b.modified - a.modified)) if (!seen.has(p.path)) { seen.add(p.path); out.push({ path: p.path, recent: false }); }
    return out;
  }, [recent, found]);

  const results = useMemo(() => {
    const q = query.trim();
    if (!q) return candidates.slice(0, LIMIT).map((x) => ({ ...x, positions: [] as number[], score: 0 }));
    const out = [];
    for (const x of candidates) {
      const m = fuzzy(q, x.path);
      if (m) out.push({ ...x, ...m });
    }
    out.sort((a, b) => b.score - a.score || Number(b.recent) - Number(a.recent) || a.path.length - b.path.length);
    return out.slice(0, LIMIT);
  }, [query, candidates]);

  return (
    <Modal visible={visible} transparent animationType="slide" onRequestClose={onClose}>
      <KeyboardAvoidingView style={styles.backdrop} behavior={Platform.OS === "ios" ? "padding" : undefined}>
        <Pressable style={{ flex: 1 }} onPress={onClose} />
        <View style={[styles.sheet, { paddingBottom: Math.max(insets.bottom, 12) }]}>
          <View style={styles.grabber} />
          <View style={styles.search}>
            <Icon name="search" size={16} color={c.faint} />
            <TextInput
              style={styles.input}
              placeholder="Find a project…"
              placeholderTextColor={c.faint}
              value={query}
              onChangeText={setQuery}
              autoCapitalize="none"
              autoCorrect={false}
              returnKeyType="go"
              onSubmitEditing={() => results[0] && onPick(results[0].path)}
            />
            {found === null && <ActivityIndicator size="small" color={c.faint} />}
          </View>
          <FlatList
            data={results}
            keyExtractor={(r) => r.path}
            keyboardShouldPersistTaps="handled"
            style={{ maxHeight: 460 }}
            ListEmptyComponent={found ? <Text style={styles.empty}>No project matches “{query}”.</Text> : undefined}
            renderItem={({ item: r, index }) => {
              const nameAt = r.path.lastIndexOf("/") + 1;
              const homeLen = r.path.match(/^\/Users\/[^/]+/)?.[0].length ?? 0;
              return (
                <>
                  {index === 0 && r.recent && !query && <Text style={styles.group}>WITH CHATS</Text>}
                  {!r.recent && !query && (index === 0 || results[index - 1].recent) && <Text style={styles.group}>ON YOUR MAC</Text>}
                  <Pressable style={({ pressed }) => [styles.row, pressed && { backgroundColor: c.hover }]} onPress={() => onPick(r.path)}>
                    <ProjectIcon client={client} path={r.path} size={30} />
                    <View style={{ flex: 1, minWidth: 0 }}>
                      <Text style={styles.name} numberOfLines={1}><Marked text={r.path.slice(nameAt)} from={nameAt} positions={r.positions} /></Text>
                      <Text style={styles.path} numberOfLines={1}>
                        {homeLen ? "~" : ""}<Marked text={r.path.slice(homeLen, nameAt - 1)} from={homeLen} positions={r.positions} />
                      </Text>
                    </View>
                  </Pressable>
                </>
              );
            }}
          />
        </View>
      </KeyboardAvoidingView>
    </Modal>
  );
}

/** Text with the fuzzy-matched characters in the accent color. */
function Marked({ text, from, positions }: { text: string; from: number; positions: number[] }) {
  if (!positions.length) return <>{text}</>;
  const hit = new Set(positions.map((p) => p - from));
  const runs: { t: string; on: boolean }[] = [];
  for (let i = 0; i < text.length; i++) {
    const on = hit.has(i);
    const last = runs[runs.length - 1];
    if (last && last.on === on) last.t += text[i];
    else runs.push({ t: text[i], on });
  }
  return <>{runs.map((r, i) => (r.on ? <Text key={i} style={{ color: c.accent, fontWeight: "700" }}>{r.t}</Text> : r.t))}</>;
}

const styles = sheet(() => ({
  backdrop: { flex: 1, backgroundColor: "rgba(0,0,0,0.55)" },
  sheet: { backgroundColor: c.panel, borderTopLeftRadius: 26, borderTopRightRadius: 26, paddingHorizontal: 12, paddingTop: 10, borderWidth: 1, borderColor: c.border },
  grabber: { alignSelf: "center", width: 38, height: 4, borderRadius: 2, backgroundColor: c.border, marginBottom: 12 },
  search: { flexDirection: "row", alignItems: "center", gap: 10, height: 46, paddingHorizontal: 14, marginHorizontal: 4, borderRadius: 14, backgroundColor: c.raised },
  input: { flex: 1, color: c.text, fontSize: 16, paddingVertical: 0 },
  group: { color: c.faint, fontSize: 11, fontWeight: "700", letterSpacing: 0.9, marginTop: 14, marginBottom: 4, marginLeft: 10 },
  row: { flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 10, paddingVertical: 9, borderRadius: 14 },
  name: { color: c.text, fontSize: 15.5, fontWeight: "600" },
  path: { color: c.faint, fontSize: 12.5, marginTop: 1 },
  empty: { color: c.faint, textAlign: "center", paddingVertical: 28 },
}));
