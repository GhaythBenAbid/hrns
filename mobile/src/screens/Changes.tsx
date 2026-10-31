import { useCallback, useEffect, useMemo, useState } from "react";
import { ActivityIndicator, FlatList, Pressable, RefreshControl, ScrollView, Text, TextInput, View } from "react-native";
import { base, type Client, type GitFile, type GitStatus } from "../api";
import { fuzzy } from "../fuzzy";
import { Icon } from "../Icon";
import { alpha, c, sheet } from "../theme";

// Uncommitted changes of a chat's project, like the desktop changes sidebar: per-file counts
// first, each file's diff fetched only when you open it, and an fzf-style filter over paths.

const MAX_LINES = 400;
/** Lines drawn on the first frame; the rest follow right after so the file opens at once. */
const FIRST_PAINT = 60;

// Diffs by repo, path and repo fingerprint: reopening a file, or scrolling back to it, is instant.
const diffs = new Map<string, string | null>();

export function Changes({ client, cwd, onBack }: { client: Client; cwd: string; onBack: () => void }) {
  const [status, setStatus] = useState<GitStatus | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [refreshing, setRefreshing] = useState(false);

  const load = useCallback(async () => {
    try {
      setStatus(await client.gitStatus(cwd));
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, [client, cwd]);

  useEffect(() => {
    load();
  }, [load]);

  const files = useMemo(() => {
    const all = status?.files ?? [];
    const q = query.trim();
    if (!q) return all.map((f) => ({ f, hits: [] as number[] }));
    const out = [];
    for (const f of all) {
      const m = fuzzy(q, f.path);
      if (m) out.push({ f, hits: m.positions, score: m.score });
    }
    return out.sort((a, b) => b.score - a.score || a.f.path.length - b.f.path.length);
  }, [status, query]);

  return (
    <View style={{ flex: 1 }}>
      <View style={styles.header}>
        <Pressable onPress={onBack} hitSlop={10} style={styles.back}>
          <Icon name="back" size={24} color={c.text} strokeWidth={2.2} />
        </Pressable>
        <View style={{ flex: 1 }}>
          <Text style={styles.title}>Uncommitted changes</Text>
          <Text style={styles.sub} numberOfLines={1}>
            {base(cwd)}
            {status ? ` · ${status.files.length} file${status.files.length === 1 ? "" : "s"}` : ""}
          </Text>
        </View>
        {status && status.files.length > 0 && (
          <View style={styles.totals}>
            <Text style={[styles.count, { color: c.green }]}>+{status.add.toLocaleString("en-US")}</Text>
            <Text style={[styles.count, { color: c.danger }]}>−{status.del.toLocaleString("en-US")}</Text>
          </View>
        )}
      </View>

      {status === undefined && !error ? (
        <ActivityIndicator color={c.accent} style={{ marginTop: 40 }} />
      ) : (
        <FlatList
          data={files}
          keyExtractor={({ f }) => f.path}
          keyboardShouldPersistTaps="handled"
          contentContainerStyle={{ padding: 12, paddingBottom: 40 }}
          refreshControl={<RefreshControl refreshing={refreshing} tintColor={c.muted} onRefresh={async () => { setRefreshing(true); await load(); setRefreshing(false); }} />}
          ListHeaderComponent={
            <>
              {error && <Text style={styles.error}>{error}</Text>}
              {status && status.files.length > 6 && (
                <View style={styles.search}>
                  <Icon name="search" size={15} color={c.faint} />
                  <TextInput style={styles.input} placeholder="Filter files…" placeholderTextColor={c.faint} value={query} onChangeText={setQuery} autoCapitalize="none" autoCorrect={false} />
                </View>
              )}
            </>
          }
          ListEmptyComponent={
            status === null ? <Text style={styles.empty}>This folder isn't a git repository.</Text>
              : status && !status.files.length ? <Text style={styles.empty}>No uncommitted changes. The working tree matches the last commit.</Text>
              : query ? <Text style={styles.empty}>No file matches “{query}”.</Text> : undefined
          }
          renderItem={({ item: { f, hits } }) => (
            <FileCard client={client} root={status!.root} file={f} hits={hits} rev={status!.rev} open={!!open[f.path]} onToggle={() => setOpen((o) => ({ ...o, [f.path]: !o[f.path] }))} />
          )}
        />
      )}
    </View>
  );
}

function FileCard({ client, root, file: f, hits, rev, open, onToggle }: { client: Client; root: string; file: GitFile; hits: number[]; rev: string; open: boolean; onToggle: () => void }) {
  const key = `${root}\0${f.path}\0${rev}`;
  const [diff, setDiff] = useState<string | null | undefined>(() => diffs.get(key));
  const [all, setAll] = useState(false);
  const [painted, setPainted] = useState(FIRST_PAINT);

  // Fetched on first open, and again when the repo changed since. The old diff stays on screen meanwhile.
  useEffect(() => {
    if (!open || f.binary) return;
    if (diffs.has(key)) return setDiff(diffs.get(key));
    let live = true;
    client.gitDiff(root, f).then((d) => {
      diffs.set(key, d);
      if (live) setDiff(d);
    }).catch(() => live && setDiff(null));
    return () => {
      live = false;
    };
  }, [open, key]); // eslint-disable-line react-hooks/exhaustive-deps

  // Paint the first screenful, then the rest on the next frame.
  useEffect(() => {
    if (!open || !diff) return;
    setPainted(FIRST_PAINT);
    const t = requestAnimationFrame(() => setPainted(Infinity));
    return () => cancelAnimationFrame(t);
  }, [open, diff]);

  const dirEnd = f.path.lastIndexOf("/") + 1;
  const rows = useMemo(() => (diff ? parsePatch(diff) : []), [diff]);
  const shown = rows.slice(0, Math.min(painted, all ? Infinity : MAX_LINES));
  const width = String(rows.reduce((m, r) => Math.max(m, r.old ?? 0, r.new ?? 0), 0)).length;
  const kind = f.untracked ? "new" : f.kind === "add" ? "added" : f.kind === "delete" ? "deleted" : f.kind === "rename" ? "renamed" : null;

  return (
    <View style={styles.card}>
      <Pressable style={styles.fileHead} onPress={onToggle}>
        <Icon name={open ? "chevronDown" : "chevronRight"} size={13} color={c.faint} strokeWidth={2.6} />
        <View style={{ flex: 1, minWidth: 0 }}>
          <Text style={styles.fileName} numberOfLines={1}><Marked text={f.path.slice(dirEnd)} from={dirEnd} hits={hits} /></Text>
          {dirEnd > 0 && <Text style={styles.fileDir} numberOfLines={1}><Marked text={f.path.slice(0, dirEnd - 1)} from={0} hits={hits} /></Text>}
        </View>
        {kind && <View style={styles.tag}><Text style={styles.tagText}>{kind}</Text></View>}
        {f.binary ? <Text style={styles.fileDir}>binary</Text> : (
          <View style={styles.totals}>
            <Text style={[styles.small, { color: c.green }]}>+{f.add}</Text>
            <Text style={[styles.small, { color: c.danger }]}>−{f.del}</Text>
          </View>
        )}
      </Pressable>
      {open && (
        <View style={styles.diff}>
          {f.binary ? <Text style={styles.note}>Binary file, no text diff.</Text>
            : diff === undefined ? <ActivityIndicator size="small" color={c.muted} style={{ padding: 12 }} />
            : diff === null ? <Text style={styles.note}>Too large to show here.</Text>
            : (
              <ScrollView horizontal showsHorizontalScrollIndicator={false}>
                <View style={{ paddingVertical: 4 }}>
                  {shown.map((r, i) => (
                    // One View + one Text per line (nested Text spans aren't native views): cheap to lay out.
                    <View key={i} style={[styles.row, r.kind === "add" && styles.rowAdd, r.kind === "del" && styles.rowDel, r.kind === "hunk" && styles.rowHunk]}>
                      <Text style={[styles.code, r.kind === "add" && { color: c.green }, r.kind === "del" && { color: c.danger }, r.kind === "hunk" && styles.hunk]}>
                        <Text style={styles.gutter}>{pad(r.old, width)} {pad(r.new, width)} </Text>
                        <Text style={r.kind === "ctx" || r.kind === "hunk" ? styles.gutter : undefined}>{r.kind === "add" ? "+ " : r.kind === "del" ? "− " : "  "}</Text>
                        {r.text || " "}
                      </Text>
                    </View>
                  ))}
                  {rows.length > shown.length && (
                    <Pressable onPress={() => setAll(true)} style={{ padding: 10 }}>
                      <Text style={{ color: c.accent, fontSize: 13, fontWeight: "600" }}>Show {rows.length - shown.length} more lines</Text>
                    </Pressable>
                  )}
                </View>
              </ScrollView>
            )}
        </View>
      )}
    </View>
  );
}

const pad = (n: number | undefined, w: number) => (n == null ? "" : String(n)).padStart(w, " ");

type Row = { kind: "add" | "del" | "ctx" | "hunk"; old?: number; new?: number; text: string };

/** Same rows as the desktop diff: old and new line numbers, a +/− marker, hunk headers showing their context. */
function parsePatch(patch: string): Row[] {
  let o = 1, n = 1;
  const out: Row[] = [];
  for (const l of patch.split("\n")) {
    const h = l.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)/);
    if (h) {
      o = +h[1];
      n = +h[2];
      out.push({ kind: "hunk", text: h[3].trim() || l });
    } else if (l.startsWith("\\")) {
      continue; // "\ No newline at end of file"
    } else if (l.startsWith("+")) out.push({ kind: "add", new: n++, text: l.slice(1) });
    else if (l.startsWith("-")) out.push({ kind: "del", old: o++, text: l.slice(1) });
    else out.push({ kind: "ctx", old: o++, new: n++, text: l.slice(1) });
  }
  return out;
}

function Marked({ text, from, hits }: { text: string; from: number; hits: number[] }) {
  if (!hits.length) return <>{text}</>;
  const set = new Set(hits.map((h) => h - from));
  const runs: { t: string; on: boolean }[] = [];
  for (let i = 0; i < text.length; i++) {
    const on = set.has(i);
    const last = runs[runs.length - 1];
    if (last && last.on === on) last.t += text[i];
    else runs.push({ t: text[i], on });
  }
  return <>{runs.map((r, i) => (r.on ? <Text key={i} style={{ color: c.accent, fontWeight: "700" }}>{r.t}</Text> : r.t))}</>;
}

const styles = sheet(() => ({
  header: { flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 12, paddingVertical: 10, borderBottomWidth: 1, borderBottomColor: c.border },
  back: { width: 34, height: 34, alignItems: "center", justifyContent: "center" },
  title: { color: c.text, fontSize: 16, fontWeight: "700" },
  sub: { color: c.faint, fontSize: 12.5, marginTop: 1 },
  totals: { flexDirection: "row", gap: 6, alignItems: "center" },
  count: { fontSize: 13.5, fontWeight: "700", fontVariant: ["tabular-nums"] },
  small: { fontSize: 12.5, fontWeight: "600", fontVariant: ["tabular-nums"] },
  search: { flexDirection: "row", alignItems: "center", gap: 9, height: 42, paddingHorizontal: 12, borderRadius: 12, backgroundColor: c.raised, marginBottom: 10 },
  input: { flex: 1, color: c.text, fontSize: 15, paddingVertical: 0 },
  card: { borderRadius: 14, borderWidth: 1, borderColor: c.border, backgroundColor: c.panel, marginBottom: 8, overflow: "hidden" },
  fileHead: { flexDirection: "row", alignItems: "center", gap: 10, paddingHorizontal: 12, paddingVertical: 11 },
  fileName: { color: c.text, fontSize: 14, fontWeight: "600", fontFamily: c.mono },
  fileDir: { color: c.faint, fontSize: 12, marginTop: 1 },
  tag: { paddingHorizontal: 6, height: 18, borderRadius: 6, backgroundColor: c.raised, justifyContent: "center" },
  tagText: { color: c.muted, fontSize: 10.5, fontWeight: "700" },
  diff: { borderTopWidth: 1, borderTopColor: c.border, backgroundColor: c.codeBg },
  row: { flexDirection: "row", alignItems: "flex-start" },
  rowAdd: { backgroundColor: alpha(c.green, 0.1) },
  rowDel: { backgroundColor: alpha(c.danger, 0.1) },
  rowHunk: { backgroundColor: alpha(c.accent, 0.07), marginVertical: 2 },
  gutter: { color: alpha(c.faint, 0.75) },
  code: { color: c.codeText, fontFamily: c.mono, fontSize: 11.5, lineHeight: 18, paddingLeft: 10, paddingRight: 16 },
  hunk: { color: c.accent, opacity: 0.85 },
  note: { color: c.faint, fontSize: 13, padding: 12 },
  empty: { color: c.faint, textAlign: "center", marginTop: 50, paddingHorizontal: 36, lineHeight: 20 },
  error: { color: c.danger, marginBottom: 10, fontSize: 13.5 },
}));
