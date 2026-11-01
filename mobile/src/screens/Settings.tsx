import { useEffect, useState } from "react";
import { ActivityIndicator, Alert, Pressable, ScrollView, Text, View } from "react-native";
import type { AccountStatus, Client, Model, Prefs } from "../api";
import { enableNotifications, notificationsEnabled } from "../notify";
import { savePrefs } from "../prefs";
import { accents, alpha, c, sheet, type Accent, type Appearance, type ThemeMode } from "../theme";

type Account = { id: string; email?: string | null; plan?: string | null };

const ACCESS = [
  { id: "read-only", label: "Read only", desc: "Read files; ask before editing or running commands." },
  { id: "auto", label: "Auto", desc: "Edit and run inside the project; ask for anything else." },
  { id: "full", label: "Full access", desc: "No sandbox and no prompts. Use with care." },
] as const;

type Props = {
  client: Client;
  host: string;
  connected: boolean;
  prefs: Prefs;
  onPrefs: (p: Prefs) => void;
  look: Appearance;
  onLook: (a: Appearance) => void;
  onUnpair: () => void;
};

const THEMES: { id: ThemeMode; label: string }[] = [
  { id: "system", label: "System" },
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" },
];

export function Settings({ client, host, connected, prefs, onPrefs, look, onLook, onUnpair }: Props) {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [statuses, setStatuses] = useState<Record<string, AccountStatus>>({});
  const [models, setModels] = useState<Model[] | null>(null);
  const [version, setVersion] = useState("");
  const account = prefs.account ?? "default";
  const [notifs, setNotifs] = useState<boolean | null>(null);
  useEffect(() => {
    notificationsEnabled().then(setNotifs);
  }, []);

  useEffect(() => {
    client.info().then((i) => {
      setVersion(i.version);
      setAccounts(i.accounts);
      for (const a of i.accounts.filter((a) => a.email)) {
        client.accountStatus(a.id).then((s) => setStatuses((cur) => ({ ...cur, [a.id]: s }))).catch(() => {});
      }
    }).catch(() => {});
  }, [client]);

  useEffect(() => {
    setModels(null);
    client.models(account).then(setModels).catch(() => setModels([]));
  }, [client, account]);

  const update = (patch: Partial<Prefs>) => {
    const next = { ...prefs, ...patch };
    onPrefs(next);
    savePrefs(next);
  };

  const model = models?.find((m) => m.id === prefs.model) ?? models?.find((m) => m.isDefault) ?? models?.[0];
  const effort = prefs.effort ?? model?.defaultReasoningEffort;
  const namer = models?.find((m) => /mini|nano|spark|flash|lite/i.test(m.id)) ?? model;
  const theme = look.theme ?? "system";
  const accent = look.accent ?? "violet";

  return (
    <ScrollView contentContainerStyle={styles.wrap}>
      <Text style={styles.h1}>Settings</Text>

      <Section title="Connection">
        <Row label={host} hint={connected ? "Connected over Tailscale" : "Reconnecting…"} right={<View style={[styles.dot, { backgroundColor: connected ? c.green : c.faint }]} />} />
        <Pressable
          onPress={() =>
            Alert.alert("Unpair this phone?", "You'll need to scan the QR code in hrns again.", [
              { text: "Cancel", style: "cancel" },
              { text: "Unpair", style: "destructive", onPress: onUnpair },
            ])
          }
        >
          <Row label="Unpair" danger />
        </Pressable>
      </Section>

      <Section title="Appearance">
        <View style={styles.themes}>
          {THEMES.map((t) => {
            const on = theme === t.id;
            return (
              <Pressable key={t.id} style={styles.themeOpt} onPress={() => onLook({ ...look, theme: t.id })} accessibilityState={{ selected: on }}>
                <View style={[styles.swatch, on && { borderColor: c.accent }]}>
                  {t.id === "system" ? (
                    <View style={{ flex: 1, flexDirection: "row" }}>
                      <View style={[styles.swatchHalf, { backgroundColor: "#f2f2f4" }]}><View style={[styles.swatchLine, { backgroundColor: "#d4d4d8" }]} /></View>
                      <View style={[styles.swatchHalf, { backgroundColor: "#111113" }]}><View style={[styles.swatchLine, { backgroundColor: "#3f3f46" }]} /></View>
                    </View>
                  ) : (
                    <View style={[styles.swatchHalf, { backgroundColor: t.id === "light" ? "#f2f2f4" : "#111113" }]}>
                      <View style={[styles.swatchLine, { backgroundColor: t.id === "light" ? "#d4d4d8" : "#3f3f46" }]} />
                      <View style={[styles.swatchLine, { width: "40%", backgroundColor: accents[accent][t.id] }]} />
                    </View>
                  )}
                </View>
                <Text style={[styles.themeLabel, on && { color: c.text, fontWeight: "700" }]}>{t.label}</Text>
              </Pressable>
            );
          })}
        </View>
        <View style={styles.accents}>
          {(Object.keys(accents) as Accent[]).map((id) => {
            const on = accent === id;
            return (
              <Pressable key={id} onPress={() => onLook({ ...look, accent: id })} hitSlop={4} accessibilityLabel={`${accents[id].label} accent`} accessibilityState={{ selected: on }}>
                <View style={[styles.dotRing, on && { borderColor: accents[id][c.scheme] }]}>
                  <View style={[styles.accentDot, { backgroundColor: accents[id][c.scheme] }]} />
                </View>
              </Pressable>
            );
          })}
        </View>
      </Section>

      <Section title="Notifications" note="Approvals and finished turns from your Mac, with Approve / Deny right in the notification.">
        <Row
          label={notifs ? "On" : "Off"}
          hint={notifs ? "You'll be notified while hrns is open or in the background." : "Get notified when Codex needs you."}
          right={
            notifs ? (
              <View style={[styles.dot, { backgroundColor: c.green }]} />
            ) : (
              <Pressable style={styles.enable} onPress={async () => setNotifs(await enableNotifications())}>
                <Text style={styles.enableText}>Enable</Text>
              </Pressable>
            )
          }
        />
      </Section>

      <Section title="Account for new chats" note="Existing chats always stay on the account they started with.">
        {accounts.filter((a) => a.email).map((a) => {
          const s = statuses[a.id]?.rateLimits;
          return (
            <Pressable key={a.id} onPress={() => update({ account: a.id, model: undefined, effort: undefined })}>
              <Row
                label={a.email ?? a.id}
                hint={`ChatGPT ${a.plan ?? ""}${s?.primary ? ` · 5h ${Math.round(s.primary.usedPercent)}%` : ""}${s?.secondary ? ` · week ${Math.round(s.secondary.usedPercent)}%` : ""}`}
                selected={a.id === account}
              />
              {s?.primary && (
                <View style={styles.meter}>
                  <View style={[styles.meterFill, { width: `${Math.min(100, s.primary.usedPercent)}%`, backgroundColor: s.primary.usedPercent > 80 ? c.danger : c.accent }]} />
                </View>
              )}
            </Pressable>
          );
        })}
      </Section>

      <Section title="Model for new chats">
        {!models ? (
          <ActivityIndicator color={c.accent} style={{ padding: 16 }} />
        ) : (
          models.map((m) => (
            <Pressable key={m.id} onPress={() => update({ model: m.id, effort: undefined })}>
              <Row label={m.displayName} hint={m.description} selected={m.id === model?.id} />
            </Pressable>
          ))
        )}
      </Section>

      {model && model.supportedReasoningEfforts.length > 0 && (
        <Section title="Reasoning effort">
          <View style={styles.chips}>
            {model.supportedReasoningEfforts.map((e) => (
              <Pressable key={e.reasoningEffort} style={[styles.chip, e.reasoningEffort === effort && styles.chipOn]} onPress={() => update({ effort: e.reasoningEffort })}>
                <Text style={[styles.chipText, e.reasoningEffort === effort && { color: c.text }]}>{e.reasoningEffort}</Text>
              </Pressable>
            ))}
          </View>
        </Section>
      )}

      <Section title="Chat titles" note="A model names chats you start from the phone after the first message. The new title scrambles in.">
        <Pressable onPress={() => update({ titleModel: "auto" })}>
          <Row label="Auto" hint={namer ? `Small and fast: ${namer.displayName}` : "Picks a small, fast model"} selected={(prefs.titleModel ?? "auto") === "auto"} />
        </Pressable>
        {models?.map((m) => (
          <Pressable key={m.id} onPress={() => update({ titleModel: m.id })}>
            <Row label={m.displayName} selected={prefs.titleModel === m.id} />
          </Pressable>
        ))}
        <Pressable onPress={() => update({ titleModel: "off" })}>
          <Row label="Off" hint="Name chats after the first line of your message." selected={prefs.titleModel === "off"} />
        </Pressable>
      </Section>

      <Section title="Permissions for new chats">
        {ACCESS.map((a) => (
          <Pressable key={a.id} onPress={() => update({ access: a.id })}>
            <Row label={a.label} hint={a.desc} selected={(prefs.access ?? "auto") === a.id} danger={a.id === "full" && prefs.access === "full"} />
          </Pressable>
        ))}
      </Section>

      <Text style={styles.about}>hrns {version ? `v${version}` : ""} · Remote control over Tailscale</Text>
    </ScrollView>
  );
}

function Section({ title, note, children }: { title: string; note?: string; children: React.ReactNode }) {
  return (
    <View style={{ marginTop: 22 }}>
      <Text style={styles.sectionTitle}>{title.toUpperCase()}</Text>
      <View style={styles.group}>{children}</View>
      {note && <Text style={styles.note}>{note}</Text>}
    </View>
  );
}

function Row({ label, hint, selected, danger, right }: { label: string; hint?: string; selected?: boolean; danger?: boolean; right?: React.ReactNode }) {
  return (
    <View style={styles.row}>
      <View style={{ flex: 1 }}>
        <Text style={[styles.label, danger && { color: c.danger }]} numberOfLines={1}>{label}</Text>
        {hint ? <Text style={styles.hint} numberOfLines={2}>{hint}</Text> : null}
      </View>
      {right ?? (selected !== undefined ? <View style={[styles.radio, selected && styles.radioOn]} /> : null)}
    </View>
  );
}

const styles = sheet(() => ({
  wrap: { padding: 20, paddingBottom: 40 },
  h1: { color: c.text, fontSize: 30, fontWeight: "700", letterSpacing: -0.5 },
  sectionTitle: { color: c.faint, fontSize: 11.5, fontWeight: "700", letterSpacing: 0.8, marginBottom: 8, marginLeft: 4 },
  group: { backgroundColor: c.panel, borderRadius: 14, borderWidth: 1, borderColor: c.border, overflow: "hidden" },
  note: { color: c.faint, fontSize: 12, marginTop: 6, marginLeft: 4 },
  row: { flexDirection: "row", alignItems: "center", gap: 12, paddingHorizontal: 14, paddingVertical: 12, borderBottomWidth: 0.5, borderBottomColor: c.border },
  label: { color: c.text, fontSize: 15, fontWeight: "600" },
  hint: { color: c.muted, fontSize: 12.5, marginTop: 2, lineHeight: 17 },
  radio: { width: 20, height: 20, borderRadius: 10, borderWidth: 1.5, borderColor: c.faint },
  radioOn: { borderWidth: 6, borderColor: c.accent },
  dot: { width: 9, height: 9, borderRadius: 5 },
  meter: { height: 3, backgroundColor: c.raised, marginHorizontal: 14, marginBottom: 10, marginTop: -4, borderRadius: 2, overflow: "hidden" },
  meterFill: { height: 3, borderRadius: 2 },
  chips: { flexDirection: "row", flexWrap: "wrap", gap: 8, padding: 12 },
  chip: { paddingHorizontal: 14, height: 34, borderRadius: 17, borderWidth: 1, borderColor: c.border, justifyContent: "center" },
  chipOn: { backgroundColor: c.hover, borderColor: c.accent },
  chipText: { color: c.muted, fontSize: 13.5, fontWeight: "600", textTransform: "capitalize" },
  enable: { height: 32, paddingHorizontal: 14, borderRadius: 10, backgroundColor: c.text, justifyContent: "center" },
  enableText: { color: c.bg, fontSize: 13, fontWeight: "700" },
  themes: { flexDirection: "row", gap: 12, padding: 14, paddingBottom: 6 },
  themeOpt: { flex: 1, alignItems: "center", gap: 7 },
  swatch: { width: "100%", height: 58, borderRadius: 12, borderWidth: 2, borderColor: c.border, overflow: "hidden" },
  swatchHalf: { flex: 1, padding: 9, gap: 6 },
  swatchLine: { height: 5, width: "75%", borderRadius: 3 },
  themeLabel: { color: c.muted, fontSize: 12.5, fontWeight: "600" },
  accents: { flexDirection: "row", flexWrap: "wrap", gap: 10, paddingHorizontal: 14, paddingTop: 8, paddingBottom: 14 },
  dotRing: { width: 34, height: 34, borderRadius: 17, borderWidth: 2, borderColor: "transparent", alignItems: "center", justifyContent: "center" },
  accentDot: { width: 24, height: 24, borderRadius: 12 },
  about: { color: c.faint, fontSize: 12, textAlign: "center", marginTop: 28 },
}));
