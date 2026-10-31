import { useEffect, useState } from "react";
import { ActivityIndicator, Alert, Pressable, ScrollView, StyleSheet, Text, View } from "react-native";
import type { AccountStatus, Client, Model, Prefs } from "../api";
import { enableNotifications, notificationsEnabled } from "../notify";
import { savePrefs } from "../prefs";
import { c } from "../theme";

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
  onUnpair: () => void;
};

export function Settings({ client, host, connected, prefs, onPrefs, onUnpair }: Props) {
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

const styles = StyleSheet.create({
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
  about: { color: c.faint, fontSize: 12, textAlign: "center", marginTop: 28 },
});
