import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ActivityIndicator, Keyboard, KeyboardAvoidingView, Platform, Pressable, ScrollView, Text, TextInput, View, type ScrollViewInstance } from "react-native";
import Markdown from "react-native-markdown-display";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { base, describeCommand, diffStat, type Approval, type ChatEvent, type Client, type GitStatus, type Live, type Message, type Part, type Prefs, type Thread } from "../api";
import { AnimatedTitle } from "../AnimatedTitle";
import { Icon, StopIcon } from "../Icon";
import { alpha, c, sheet, themed } from "../theme";

const newId = () =>
  "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (ch) => {
    const r = (Math.random() * 16) | 0;
    return (ch === "x" ? r : (r & 0x3) | 0x8).toString(16);
  });

type Props = { client: Client; threadId: string | null; cwd: string | null; prefs: Prefs; onBack: () => void; onChanges: (cwd: string) => void; onTerminal: (cwd: string) => void };

export function Chat({ client, threadId, cwd, prefs, onBack, onChanges, onTerminal }: Props) {
  // A new chat gets its id up front so streamed events can be matched immediately.
  const [id] = useState(threadId ?? newId());
  const [thread, setThread] = useState<Thread | null>(null);
  const [live, setLive] = useState<Live | null>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [git, setGit] = useState<GitStatus | null>(null);
  const liveRef = useRef<Live | null>(null);
  const flush = useRef<ReturnType<typeof setTimeout> | null>(null);
  const scroller = useRef<ScrollViewInstance>(null);
  const atBottom = useRef(true);
  // Bottom inset only while the keyboard is closed; otherwise the composer floats above it with a gap.
  const insets = useSafeAreaInsets();
  const [keyboard, setKeyboard] = useState(false);
  useEffect(() => {
    const show = Keyboard.addListener("keyboardDidShow", () => setKeyboard(true));
    const hide = Keyboard.addListener("keyboardDidHide", () => setKeyboard(false));
    return () => {
      show.remove();
      hide.remove();
    };
  }, []);

  const setLiveNow = (l: Live | null) => {
    liveRef.current = l;
    setLive(l ? { ...l, parts: l.parts.map((p) => ({ ...p })), approvals: [...l.approvals] } : null);
  };

  const load = useCallback(async () => {
    if (!threadId && !thread) return;
    try {
      const r = await client.thread(id);
      setThread(r.thread);
      setLiveNow(r.live);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, [client, id]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    load();
  }, [load]);

  // Uncommitted changes in the chat's project, refreshed after every turn.
  const folder = thread?.cwd ?? cwd;
  const loadGit = useCallback(() => {
    if (folder) client.gitStatus(folder).then(setGit).catch(() => {});
  }, [client, folder]);
  useEffect(loadGit, [loadGit]);

  useEffect(
    () =>
      client.on((e) => {
        if (e.type === "resync") return void load();
        if (e.type === "thread-title" && e.payload.threadId === id) return setThread((t) => (t ? { ...t, title: e.payload.title } : t));
        if (e.type !== "chat" || e.payload.threadId !== id) return;
        apply(e.payload);
      }),
    [client, id, load],
  );

  function apply(ev: ChatEvent) {
    if (ev.kind === "done") {
      loadGit();
      liveRef.current = null;
      setLive(null);
      if (ev.message) setThread((t) => (t && !t.messages.some((m) => m.id === ev.message!.id) ? { ...t, messages: [...t.messages, ev.message!] } : t));
      return;
    }
    const l = liveRef.current ?? { parts: [], approvals: [] };
    liveRef.current = l;
    if (ev.kind === "part") {
      const i = l.parts.findIndex((p) => p.id === ev.part.id);
      if (i >= 0) l.parts[i] = ev.part;
      else l.parts.push(ev.part);
    } else if (ev.kind === "delta") {
      let p = l.parts.find((x) => x.id === ev.partId) as Record<string, any> | undefined;
      if (!p) {
        p = { type: ev.field === "output" ? "command" : "text", id: ev.partId, [ev.field]: "", status: "inProgress", command: "" };
        l.parts.push(p as Part);
      }
      p[ev.field] = (p[ev.field] ?? "") + ev.text;
    } else if (ev.kind === "approval") l.approvals.push(ev.approval);
    else if (ev.kind === "approvalResolved") l.approvals = l.approvals.filter((a) => a.key !== ev.key);
    else if (ev.kind === "notice") l.notice = ev.message;
    // Re-render at most ~12x/s while streaming.
    if (!flush.current) {
      flush.current = setTimeout(() => {
        flush.current = null;
        if (liveRef.current) setLiveNow(liveRef.current);
      }, 80);
    }
  }

  async function send() {
    const body = text.trim();
    if (!body || live) return;
    setText("");
    setError(null);
    const msg: Message = { id: newId(), role: "user", text: body, createdAt: Date.now() };
    setThread((t) => (t ? { ...t, messages: [...t.messages, msg] } : { id, title: body.slice(0, 48), model: "", access: "auto", cwd, messages: [msg], updatedAt: Date.now() }));
    setLiveNow({ parts: [], approvals: [] });
    atBottom.current = true;
    try {
      // Phone defaults apply only when starting a chat; existing chats keep their own settings.
      const isNew = !thread || thread.messages.length === 0 || !thread.model;
      const { titleModel, ...defaults } = prefs;
      const t = await client.send({ threadId: id, text: body, cwd: thread?.cwd ?? cwd, ...(isNew ? defaults : {}) });
      setThread((cur) => ({ ...t, messages: cur?.messages ?? t.messages }));
      if (isNew) nameChat(titleModel);
    } catch (e) {
      setLiveNow(null);
      setError((e as Error).message);
    }
  }

  /** Same as the desktop: a small, fast model names the chat; the new title scrambles in. */
  async function nameChat(choice = "auto") {
    if (choice === "off") return;
    try {
      const models = await client.models(prefs.account ?? "default");
      const m = models.find((x) => x.id === choice) ?? models.find((x) => /mini|nano|spark|flash|lite/i.test(x.id)) ?? models.find((x) => x.isDefault) ?? models[0];
      if (!m) return;
      const effort = ["none", "minimal", "low"].find((e) => m.supportedReasoningEfforts.some((s) => s.reasoningEffort === e)) ?? null;
      const title = await client.title(id, m.id, effort);
      setThread((t) => (t ? { ...t, title } : t));
    } catch {
      // Keep the first-line title.
    }
  }

  const busy = !!live;
  const access = thread?.access;
  const title = thread?.title ?? "New chat";

  return (
    <KeyboardAvoidingView style={styles.wrap} behavior={Platform.OS === "ios" ? "padding" : undefined} keyboardVerticalOffset={0}>
      <View style={styles.header}>
        <Pressable onPress={onBack} hitSlop={10} style={styles.back}>
          <Icon name="back" size={24} color={c.text} strokeWidth={2.2} />
        </Pressable>
        <View style={{ flex: 1 }}>
          {/* The id only becomes stable once the chat has loaded, so the placeholder → real title swap doesn't animate. */}
          <AnimatedTitle style={styles.title} text={title} id={thread ? id : undefined} />
          <Text style={styles.sub} numberOfLines={1}>
            {base(folder)}{thread?.model ? ` · ${thread.model.replace(/^gpt-/i, "")}` : ""}
            {access ? " · " : ""}
            {access ? <Text style={access === "full" ? { color: c.accent, fontWeight: "600" } : undefined}>{access === "full" ? "Full access" : access === "read-only" ? "Read only" : "Auto"}</Text> : null}
          </Text>
        </View>
        {folder && (
          <Pressable style={({ pressed }) => [styles.termBtn, pressed && { backgroundColor: c.hover }]} onPress={() => onTerminal(folder)} hitSlop={6} accessibilityLabel="Open a terminal in this project">
            <Icon name="terminal" size={18} color={c.muted} />
          </Pressable>
        )}
        {folder && git && git.files.length > 0 && (
          <Pressable style={({ pressed }) => [styles.changes, pressed && { backgroundColor: c.hover }]} onPress={() => onChanges(folder)} hitSlop={6} accessibilityLabel={`${git.files.length} uncommitted files`}>
            <Icon name="branch" size={14} color={c.muted} />
            <Text style={[styles.changesNum, { color: c.green }]}>+{git.add}</Text>
            <Text style={[styles.changesNum, { color: c.danger }]}>−{git.del}</Text>
          </Pressable>
        )}
      </View>

      <ScrollView
        ref={scroller}
        style={{ flex: 1 }}
        contentContainerStyle={styles.content}
        keyboardShouldPersistTaps="handled"
        onScroll={(e) => {
          const { contentOffset, contentSize, layoutMeasurement } = e.nativeEvent;
          atBottom.current = contentSize.height - contentOffset.y - layoutMeasurement.height < 120;
        }}
        scrollEventThrottle={100}
        onContentSizeChange={() => atBottom.current && scroller.current?.scrollToEnd({ animated: false })}
      >
        {!thread && !threadId && <Text style={styles.empty}>What should we build in {base(cwd)}?</Text>}
        {thread?.messages.map((m) => <MessageView key={m.id} msg={m} />)}
        {live && <MessageView msg={{ id: "live", role: "assistant", text: "", parts: live.parts, createdAt: 0 }} live notice={live.notice} />}
        {live?.approvals.map((a) => <ApprovalCard key={a.key} a={a} onDecide={(d) => client.approve(a.key, d).catch((e) => setError(e.message))} />)}
        {error && <Text style={styles.error}>{error}</Text>}
      </ScrollView>

      <View style={[styles.composer, { marginBottom: keyboard ? 8 : Math.max(insets.bottom, 12) }]}>
        <TextInput
          style={styles.input}
          placeholder={busy ? "Codex is working…" : "Ask anything…"}
          placeholderTextColor={c.faint}
          multiline
          value={text}
          onChangeText={setText}
        />
        {busy ? (
          <Pressable style={[styles.send, { backgroundColor: c.raised }]} onPress={() => client.stop(id).catch(() => {})}>
            <StopIcon size={14} color={c.text} />
          </Pressable>
        ) : (
          <Pressable style={[styles.send, !text.trim() && { opacity: 0.3 }]} disabled={!text.trim()} onPress={send}>
            <Icon name="send" size={18} color={c.bg} strokeWidth={2.6} />
          </Pressable>
        )}
      </View>
    </KeyboardAvoidingView>
  );
}

// ---------- message rendering ----------

type Block = { kind: "part"; part: Part } | { kind: "group"; id: string; parts: Part[] };

function MessageView({ msg, live, notice }: { msg: Message; live?: boolean; notice?: string }) {
  if (msg.role === "user") {
    return (
      <View style={styles.userRow}>
        <View style={styles.bubble}>
          <Text style={styles.bubbleText} selectable>{msg.text}</Text>
        </View>
      </View>
    );
  }
  const parts = msg.parts ?? (msg.text ? [{ type: "text" as const, id: "t", text: msg.text }] : []);
  const blocks: Block[] = [];
  for (const p of parts) {
    if (p.type === "command" || p.type === "tool" || p.type === "reasoning") {
      const last = blocks[blocks.length - 1];
      if (last?.kind === "group") last.parts.push(p);
      else blocks.push({ kind: "group", id: p.id, parts: [p] });
    } else if (p.type !== "text" || p.text) blocks.push({ kind: "part", part: p });
  }
  const lastIsGroup = blocks[blocks.length - 1]?.kind === "group";

  return (
    <View style={{ marginBottom: 6 }}>
      {blocks.map((b, i) =>
        b.kind === "group" ? <Steps key={b.id} parts={b.parts} live={!!live && i === blocks.length - 1} /> : <PartView key={b.part.id} part={b.part} />,
      )}
      {live && !lastIsGroup && (
        <View style={styles.thinking}>
          <ActivityIndicator size="small" color={c.muted} />
          <Text style={styles.thinkingText}>{notice ?? "Working…"}</Text>
        </View>
      )}
      {msg.error ? <Text style={msg.error === "Stopped" ? styles.stopped : styles.errorBox}>{msg.error}</Text> : null}
    </View>
  );
}

function Steps({ parts, live }: { parts: Part[]; live: boolean }) {
  const [open, setOpen] = useState(false);
  const commands = parts.filter((p) => p.type === "command").length;
  const tools = parts.filter((p) => p.type === "tool").length;
  const failed = parts.filter((p) => p.type === "command" && p.exitCode != null && p.exitCode !== 0).length;
  const summary = [commands && `${commands} step${commands > 1 ? "s" : ""}`, tools && `${tools} tool call${tools > 1 ? "s" : ""}`].filter(Boolean).join(", ");
  const shown = open || live;
  return (
    <View style={{ marginVertical: 6 }}>
      <Pressable style={styles.stepsHead} onPress={() => setOpen(!open)} hitSlop={6}>
        {live ? <ActivityIndicator size="small" color={c.accent} /> : null}
        <Text style={styles.stepsVerb}>{live ? "Working" : commands || tools ? "Worked" : "Thought"}</Text>
        {summary ? <Text style={styles.stepsRest}>{summary}</Text> : null}
        {failed ? <Text style={{ color: c.danger, fontSize: 12.5 }}>{failed} failed</Text> : null}
        <Icon name={shown ? "chevronDown" : "chevronRight"} size={13} color={c.faint} strokeWidth={2.6} />
      </Pressable>
      {shown && (
        <View style={styles.stepsBody}>
          {parts.map((p) =>
            p.type === "reasoning" ? (
              p.text ? <Text key={p.id} style={styles.reason} numberOfLines={open ? undefined : 1}>{p.text.replace(/\*\*/g, "")}</Text> : null
            ) : p.type === "command" ? (
              <View key={p.id} style={styles.stepRow}>
                <Text style={[styles.stepText, p.exitCode != null && p.exitCode !== 0 && { color: c.danger }]} numberOfLines={1}>
                  {p.status === "inProgress" ? "… " : ""}{describeCommand(p)}
                </Text>
              </View>
            ) : p.type === "tool" ? (
              <Text key={p.id} style={styles.stepText} numberOfLines={1}>Called {p.title}</Text>
            ) : null,
          )}
        </View>
      )}
    </View>
  );
}

function PartView({ part }: { part: Part }) {
  if (part.type === "text" || part.type === "plan") {
    return <Markdown style={md()}>{part.text}</Markdown>;
  }
  if (part.type === "fileChange") {
    return (
      <View style={{ marginVertical: 4 }}>
        {part.changes.map((ch) => {
          const s = diffStat(ch);
          return (
            <Text key={ch.path} style={styles.edit}>
              <Text style={styles.editVerb}>{ch.kind === "add" ? "Created" : ch.kind === "delete" ? "Deleted" : "Edited"} </Text>
              <Text style={styles.editFile}>{base(ch.path)} </Text>
              <Text style={{ color: c.green }}>+{s.add} </Text>
              <Text style={{ color: c.danger }}>−{s.del}</Text>
            </Text>
          );
        })}
      </View>
    );
  }
  return null;
}

function ApprovalCard({ a, onDecide }: { a: Approval; onDecide: (d: "accept" | "acceptForSession" | "decline") => void }) {
  const [sent, setSent] = useState(false);
  const decide = (d: "accept" | "acceptForSession" | "decline") => {
    setSent(true);
    onDecide(d);
  };
  return (
    <View style={styles.card}>
      <Text style={styles.cardTitle}>{a.kind === "command" ? "Allow Codex to run this command?" : a.kind === "fileChange" ? "Allow Codex to make these changes?" : "Allow extra permissions?"}</Text>
      {a.reason ? <Text style={styles.cardReason}>{a.reason}</Text> : null}
      {a.command ? <Text style={styles.cardCmd} selectable>$ {a.command}</Text> : null}
      {a.changes?.map((ch) => <Text key={ch.path} style={styles.cardFile}>{ch.kind}  {base(ch.path)}</Text>)}
      <View style={styles.cardActions}>
        <Pressable disabled={sent} style={[styles.approve, sent && { opacity: 0.5 }]} onPress={() => decide("accept")}>
          <Text style={styles.approveText}>Approve</Text>
        </Pressable>
        <Pressable disabled={sent} style={[styles.ghost, sent && { opacity: 0.5 }]} onPress={() => decide("acceptForSession")}>
          <Text style={styles.ghostText}>Always</Text>
        </Pressable>
        <Pressable disabled={sent} style={[styles.ghost, sent && { opacity: 0.5 }]} onPress={() => decide("decline")}>
          <Text style={[styles.ghostText, { color: c.danger }]}>Deny</Text>
        </Pressable>
      </View>
    </View>
  );
}

const md = themed(() => ({
  body: { color: c.text, fontSize: 15.5, lineHeight: 23 },
  heading1: { color: c.text, fontSize: 20, fontWeight: "700", marginTop: 10 },
  heading2: { color: c.text, fontSize: 18, fontWeight: "700", marginTop: 10 },
  heading3: { color: c.text, fontSize: 16, fontWeight: "700", marginTop: 8 },
  strong: { fontWeight: "700" },
  link: { color: c.accent },
  code_inline: { backgroundColor: c.raised, color: c.text, fontFamily: c.mono, fontSize: 13, borderRadius: 4, paddingHorizontal: 4 },
  fence: { backgroundColor: c.codeBg, color: c.codeText, fontFamily: c.mono, fontSize: 12.5, borderRadius: 10, borderWidth: 1, borderColor: c.border, padding: 12 },
  code_block: { backgroundColor: c.codeBg, color: c.codeText, fontFamily: c.mono, fontSize: 12.5, borderRadius: 10, padding: 12 },
  blockquote: { backgroundColor: "transparent", borderLeftColor: c.border, borderLeftWidth: 3, paddingLeft: 10 },
  table: { borderColor: c.border, borderRadius: 8 },
  th: { padding: 6, borderColor: c.border },
  td: { padding: 6, borderColor: c.border },
  tr: { borderColor: c.border },
  hr: { backgroundColor: c.border },
  bullet_list_icon: { color: c.muted },
  ordered_list_icon: { color: c.muted },
}));

const styles = sheet(() => ({
  wrap: { flex: 1 },
  header: { flexDirection: "row", alignItems: "center", gap: 8, paddingHorizontal: 12, paddingVertical: 10, borderBottomWidth: 1, borderBottomColor: c.border },
  termBtn: { width: 34, height: 34, borderRadius: 17, alignItems: "center", justifyContent: "center" },
  changes: { flexDirection: "row", alignItems: "center", gap: 5, height: 30, paddingHorizontal: 10, borderRadius: 15, borderWidth: 1, borderColor: c.border },
  changesNum: { fontSize: 12, fontWeight: "700", fontVariant: ["tabular-nums"] },
  back: { width: 34, height: 34, alignItems: "center", justifyContent: "center" },
  backText: { color: c.text, fontSize: 30, marginTop: -4 },
  title: { color: c.text, fontSize: 16, fontWeight: "700" },
  sub: { color: c.faint, fontSize: 12.5, marginTop: 1 },
  content: { padding: 16, paddingBottom: 24 },
  empty: { color: c.muted, fontSize: 20, fontWeight: "600", textAlign: "center", marginTop: 80 },
  userRow: { alignItems: "flex-end", marginTop: 18, marginBottom: 10 },
  bubble: { maxWidth: "85%", backgroundColor: c.bubble, borderRadius: 20, borderBottomRightRadius: 6, paddingHorizontal: 14, paddingVertical: 10 },
  bubbleText: { color: c.text, fontSize: 15.5, lineHeight: 22 },
  thinking: { flexDirection: "row", alignItems: "center", gap: 8, marginVertical: 6 },
  thinkingText: { color: c.muted, fontSize: 14 },
  stopped: { color: c.muted, fontSize: 13, marginTop: 4 },
  errorBox: { color: c.danger, backgroundColor: c.dangerBg, padding: 10, borderRadius: 10, marginTop: 6, overflow: "hidden" },
  error: { color: c.danger, textAlign: "center", marginTop: 12 },
  stepsHead: { flexDirection: "row", alignItems: "center", gap: 8 },
  stepsVerb: { color: c.muted, fontSize: 14, fontWeight: "700" },
  stepsRest: { color: c.faint, fontSize: 14 },
  chev: { color: c.faint, fontSize: 14 },
  stepsBody: { marginTop: 6, marginLeft: 4, paddingLeft: 12, borderLeftWidth: 1, borderLeftColor: c.border, gap: 6 },
  stepRow: { flexDirection: "row" },
  stepText: { color: c.muted, fontSize: 13, fontFamily: c.mono, flexShrink: 1 },
  reason: { color: c.faint, fontSize: 13, fontStyle: "italic" },
  edit: { fontSize: 14, marginVertical: 2 },
  editVerb: { color: c.muted, fontWeight: "600" },
  editFile: { color: c.text, fontFamily: c.mono, fontSize: 13 },
  card: { marginVertical: 10, padding: 14, borderRadius: 16, borderWidth: 1, borderColor: alpha(c.accent, 0.4), backgroundColor: alpha(c.accent, 0.07) },
  cardTitle: { color: c.text, fontSize: 15, fontWeight: "700" },
  cardReason: { color: c.muted, fontSize: 13.5, marginTop: 4 },
  cardCmd: { color: c.text, fontFamily: c.mono, fontSize: 12.5, backgroundColor: c.codeBg, padding: 10, borderRadius: 10, marginTop: 10, overflow: "hidden" },
  cardFile: { color: c.muted, fontFamily: c.mono, fontSize: 12.5, marginTop: 6 },
  cardActions: { flexDirection: "row", gap: 8, marginTop: 14 },
  approve: { flex: 1, height: 42, borderRadius: 12, backgroundColor: c.text, alignItems: "center", justifyContent: "center" },
  approveText: { color: c.bg, fontWeight: "700", fontSize: 15 },
  ghost: { paddingHorizontal: 16, height: 42, borderRadius: 12, borderWidth: 1, borderColor: c.border, alignItems: "center", justifyContent: "center" },
  ghostText: { color: c.text, fontWeight: "600", fontSize: 14.5 },
  composer: { flexDirection: "row", alignItems: "flex-end", gap: 8, margin: 12, padding: 8, paddingLeft: 14, borderRadius: 22, borderWidth: 1, borderColor: c.border, backgroundColor: c.panel },
  input: { flex: 1, color: c.text, fontSize: 15.5, maxHeight: 140, paddingVertical: 8 },
  send: { width: 38, height: 38, borderRadius: 19, backgroundColor: c.text, alignItems: "center", justifyContent: "center" },
  sendIcon: { color: c.bg, fontSize: 20, fontWeight: "800", marginTop: -2 },
  stopIcon: { width: 12, height: 12, borderRadius: 3, backgroundColor: c.text },
}));
