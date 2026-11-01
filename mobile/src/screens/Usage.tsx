import { useCallback, useEffect, useMemo, useState } from "react";
import { ActivityIndicator, Pressable, RefreshControl, ScrollView, Text, View } from "react-native";
import type { Client, Price, UsageRow } from "../api";
import { alpha, c, sheet } from "../theme";

const RANGES = [7, 30, 90] as const;
// Same fixed categorical order as the desktop Usage page, stepped for each theme.
const SERIES_DARK = ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"];
const SERIES_LIGHT = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"];

type Agg = { cost: number; tokens: number; input: number; cached: number; output: number; reasoning: number; requests: number; savings: number; pricedTokens: number };
const empty = (): Agg => ({ cost: 0, tokens: 0, input: 0, cached: 0, output: 0, reasoning: 0, requests: 0, savings: 0, pricedTokens: 0 });

const usd = (n: number) => `$${n.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
const tok = (n: number) => (n >= 1e9 ? `${(n / 1e9).toFixed(2)}B` : n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : String(n));
const dayKey = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

export function Usage({ client, accountLabel }: { client: Client; accountLabel: (id: string) => string }) {
  const [days, setDays] = useState<(typeof RANGES)[number]>(30);
  const [rows, setRows] = useState<UsageRow[] | null>(null);
  const [prices, setPrices] = useState<Record<string, Price>>({});
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [metric, setMetric] = useState<"cost" | "tokens">("cost");
  const [selected, setSelected] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const r = await client.usage(days);
      setPrices(await client.pricing([...new Set(r.map((x) => x.model))]));
      setRows(r);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, [client, days]);

  useEffect(() => {
    setRows(null);
    load();
  }, [load]);

  const data = useMemo(() => {
    const keys: string[] = [];
    const now = new Date();
    for (let i = days - 1; i >= 0; i--) keys.push(dayKey(new Date(now.getFullYear(), now.getMonth(), now.getDate() - i)));
    const inRange = new Set(keys);
    const total = empty();
    const bySource = new Map<string, Agg>();
    const byModel = new Map<string, Agg>();
    const byDay = new Map<string, Map<string, number>>();
    const dayTotal = new Map<string, Agg>();
    for (const r of rows ?? []) {
      const day = dayKey(new Date(`${r.hour}:00:00Z`));
      if (!inRange.has(day)) continue;
      const p = prices[r.model];
      const priced = p?.input != null && p?.output != null;
      const cost = priced ? ((r.input - r.cached) * p!.input! + r.cached * (p!.cached ?? p!.input!) + r.output * p!.output!) / 1e6 : 0;
      const savings = priced ? (r.cached * (p!.input! - (p!.cached ?? p!.input!))) / 1e6 : 0;
      const t = r.input + r.output;
      for (const [map, key] of [[bySource, r.source], [byModel, r.model], [dayTotal, day]] as const) {
        if (!map.has(key)) map.set(key, empty());
      }
      for (const a of [total, bySource.get(r.source)!, byModel.get(r.model)!, dayTotal.get(day)!]) {
        a.cost += cost; a.tokens += t; a.input += r.input; a.cached += r.cached; a.output += r.output;
        a.reasoning += r.reasoning; a.requests += r.requests; a.savings += savings; if (priced) a.pricedTokens += t;
      }
      if (!byDay.has(day)) byDay.set(day, new Map());
      const m = byDay.get(day)!;
      m.set(r.source, (m.get(r.source) ?? 0) + (metric === "cost" ? cost : t));
    }
    const sources = [...bySource.keys()].sort((a, b) => (a === "codex" ? -1 : b === "codex" ? 1 : a.localeCompare(b)));
    return { keys, total, bySource, byModel, byDay, dayTotal, sources };
  }, [rows, prices, days, metric]);

  const SERIES = c.scheme === "light" ? SERIES_LIGHT : SERIES_DARK;
  const color = (s: string) => SERIES[Math.max(0, data.sources.indexOf(s)) % SERIES.length];
  const sourceName = (s: string) => (s === "codex" ? "Codex CLI & apps" : `hrns · ${accountLabel(s)}`);
  const maxDay = Math.max(...data.keys.map((k) => [...(data.byDay.get(k)?.values() ?? [])].reduce((a, b) => a + b, 0)), 0.0001);
  const sel = selected ? data.dayTotal.get(selected) : null;

  return (
    <ScrollView
      contentContainerStyle={styles.wrap}
      refreshControl={<RefreshControl refreshing={refreshing} tintColor={c.muted} onRefresh={async () => { setRefreshing(true); await load(); setRefreshing(false); }} />}
    >
      <Text style={styles.h1}>Usage</Text>

      <View style={styles.seg}>
        {RANGES.map((r) => (
          <Pressable key={r} style={[styles.segBtn, days === r && styles.segOn]} onPress={() => setDays(r)}>
            <Text style={[styles.segText, days === r && { color: c.text }]}>{r} days</Text>
          </Pressable>
        ))}
      </View>

      {error && <Text style={styles.error}>{error}</Text>}
      {!rows && !error ? (
        <ActivityIndicator color={c.accent} style={{ marginTop: 40 }} />
      ) : (
        <>
          <Text style={styles.eyebrow}>RAW TOKEN COST</Text>
          <Text style={styles.big}>{usd(data.total.cost)}<Text style={{ color: c.muted }}>*</Text></Text>
          <Text style={styles.foot}>* if billed at full API rate · covered by your ChatGPT plan</Text>

          <View style={{ gap: 14, marginTop: 20 }}>
            {data.sources.map((s) => {
              const a = data.bySource.get(s)!;
              const share = data.total.cost ? a.cost / data.total.cost : 0;
              return (
                <View key={s}>
                  <View style={styles.srcTop}>
                    <View style={[styles.key, { backgroundColor: color(s) }]} />
                    <Text style={styles.srcName} numberOfLines={1}>{sourceName(s)}</Text>
                    <Text style={styles.srcVal}>{usd(a.cost)}</Text>
                  </View>
                  <View style={styles.track}><View style={[styles.fill, { width: `${share * 100}%`, backgroundColor: color(s) }]} /></View>
                  <Text style={styles.srcSub}>{(share * 100).toFixed(1)}% of cost · {tok(a.tokens)} tokens</Text>
                </View>
              );
            })}
          </View>

          <View style={styles.chartHead}>
            <Text style={styles.h2}>Daily {metric}</Text>
            <View style={styles.segSmall}>
              {(["cost", "tokens"] as const).map((m) => (
                <Pressable key={m} style={[styles.segBtnSmall, metric === m && styles.segOn]} onPress={() => setMetric(m)}>
                  <Text style={[styles.segTextSmall, metric === m && { color: c.text }]}>{m.toUpperCase()}</Text>
                </Pressable>
              ))}
            </View>
          </View>
          <View style={styles.chart}>
            {data.keys.map((k) => {
              const stack = data.byDay.get(k);
              return (
                <Pressable key={k} style={styles.barSlot} onPress={() => setSelected(selected === k ? null : k)}>
                  <View style={[styles.bar, selected && selected !== k && { opacity: 0.4 }]}>
                    {[...data.sources].reverse().map((s) => {
                      const v = stack?.get(s) ?? 0;
                      return v > 0 ? <View key={s} style={{ height: `${(v / maxDay) * 100}%`, backgroundColor: color(s), marginTop: 1.5 }} /> : null;
                    })}
                  </View>
                </Pressable>
              );
            })}
          </View>
          <Text style={styles.chartFoot}>
            {sel && selected
              ? `${new Date(`${selected}T12:00:00`).toDateString()} · ${metric === "cost" ? usd(sel.cost) : tok(sel.tokens)} · ${sel.requests} requests`
              : "Tap a bar for that day's details"}
          </Text>

          <View style={styles.grid}>
            {[
              ["Processed", tok(data.total.tokens), `${tok(data.total.requests)} requests`],
              ["Cached input", tok(data.total.cached), `${data.total.input ? ((data.total.cached / data.total.input) * 100).toFixed(1) : 0}% of input`],
              ["Output", tok(data.total.output), `incl. ${tok(data.total.reasoning)} reasoning`],
              ["Cache savings", usd(data.total.savings), data.total.cost ? `${(data.total.savings / data.total.cost).toFixed(1)}× raw cost` : "—"],
            ].map(([l, v, h]) => (
              <View key={l} style={styles.stat}>
                <Text style={styles.statL}>{l}</Text>
                <Text style={styles.statV}>{v}</Text>
                <Text style={styles.statH}>{h}</Text>
              </View>
            ))}
          </View>

          <Text style={[styles.h2, { marginTop: 24, marginBottom: 6 }]}>By model</Text>
          {[...data.byModel.entries()].sort((a, b) => b[1].cost - a[1].cost).map(([m, a]) => (
            <View key={m} style={styles.tableRow}>
              <Text style={styles.model} numberOfLines={1}>{m}</Text>
              <Text style={styles.cell}>{a.pricedTokens ? usd(a.cost) : "—"}</Text>
              <Text style={[styles.cell, { color: c.muted, width: 60 }]}>{tok(a.tokens)}</Text>
            </View>
          ))}
          <Text style={styles.note}>Prices from models.dev. Includes Codex CLI/IDE/app sessions on your Mac and chats run through hrns.</Text>
        </>
      )}
    </ScrollView>
  );
}

const styles = sheet(() => ({
  wrap: { padding: 20, paddingBottom: 40 },
  h1: { color: c.text, fontSize: 30, fontWeight: "700", letterSpacing: -0.5, marginBottom: 14 },
  h2: { color: c.text, fontSize: 15, fontWeight: "700" },
  seg: { flexDirection: "row", backgroundColor: c.panel, borderRadius: 12, borderWidth: 1, borderColor: c.border, padding: 3, marginBottom: 22 },
  segBtn: { flex: 1, height: 34, borderRadius: 9, alignItems: "center", justifyContent: "center" },
  segOn: { backgroundColor: c.hover },
  segText: { color: c.muted, fontSize: 13.5, fontWeight: "600" },
  segSmall: { flexDirection: "row", backgroundColor: c.panel, borderRadius: 9, borderWidth: 1, borderColor: c.border, padding: 2 },
  segBtnSmall: { paddingHorizontal: 10, height: 26, borderRadius: 7, alignItems: "center", justifyContent: "center" },
  segTextSmall: { color: c.muted, fontSize: 11, fontWeight: "700", letterSpacing: 0.5 },
  error: { color: c.danger, marginBottom: 10 },
  eyebrow: { color: c.muted, fontSize: 11.5, letterSpacing: 1.2 },
  big: { color: c.text, fontSize: 42, fontWeight: "800", letterSpacing: -1, marginTop: 4 },
  foot: { color: c.muted, fontSize: 12.5 },
  srcTop: { flexDirection: "row", alignItems: "center", gap: 8 },
  key: { width: 10, height: 10, borderRadius: 3 },
  srcName: { flex: 1, color: c.text, fontSize: 14.5 },
  srcVal: { color: c.text, fontSize: 14.5, fontWeight: "700" },
  track: { height: 3, borderRadius: 2, backgroundColor: c.raised, marginTop: 8, marginBottom: 5, overflow: "hidden" },
  fill: { height: 3, borderRadius: 2 },
  srcSub: { color: c.muted, fontSize: 12.5 },
  chartHead: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", marginTop: 28, marginBottom: 12 },
  chart: { height: 170, flexDirection: "row", alignItems: "flex-end", gap: 2, borderBottomWidth: 1, borderBottomColor: c.border },
  barSlot: { flex: 1, height: "100%", justifyContent: "flex-end" },
  bar: { justifyContent: "flex-end", height: "100%", borderTopLeftRadius: 3, borderTopRightRadius: 3, overflow: "hidden" },
  chartFoot: { color: c.faint, fontSize: 12, marginTop: 8, textAlign: "center" },
  grid: { flexDirection: "row", flexWrap: "wrap", marginTop: 22, borderWidth: 1, borderColor: c.border, borderRadius: 14, backgroundColor: c.panel, overflow: "hidden" },
  stat: { width: "50%", padding: 14, borderColor: c.border, borderRightWidth: 0.5, borderBottomWidth: 0.5 },
  statL: { color: c.muted, fontSize: 12.5 },
  statV: { color: c.text, fontSize: 20, fontWeight: "700", marginVertical: 4 },
  statH: { color: c.faint, fontSize: 11.5 },
  tableRow: { flexDirection: "row", alignItems: "center", paddingVertical: 11, borderBottomWidth: 0.5, borderBottomColor: c.border, gap: 10 },
  model: { flex: 1, color: c.text, fontFamily: c.mono, fontSize: 13 },
  cell: { color: c.text, fontSize: 13.5, textAlign: "right", width: 80 },
  note: { color: c.faint, fontSize: 12, marginTop: 16, lineHeight: 18 },
}));
