<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { app } from "./store.svelte";

  type Row = { hour: string; source: string; model: string; input: number; cached: number; output: number; reasoning: number; requests: number };

  const RANGES = [7, 30, 90] as const;
  // Reference categorical palette, dark steps, fixed order (validated adjacent-pair CVD-safe).
  const SERIES = ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"];

  let days = $state<(typeof RANGES)[number]>(30);
  let rows = $state<Row[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let metric = $state<"cost" | "tokens">("cost");
  let breakdown = $state<"model" | "day">("model");

  async function load() {
    loading = true;
    error = null;
    try {
      const r = await invoke<Row[]>("usage_report", { days });
      await app.loadPricing([...new Set(r.map((x) => x.model))]);
      rows = r;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    days;
    load();
  });

  // ---- derived data ----
  const localDay = (hour: string) => {
    const d = new Date(`${hour}:00:00Z`);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  };
  const dayKeys = $derived.by(() => {
    const out: string[] = [];
    const d = new Date();
    for (let i = days - 1; i >= 0; i--) {
      const x = new Date(d.getFullYear(), d.getMonth(), d.getDate() - i);
      out.push(`${x.getFullYear()}-${String(x.getMonth() + 1).padStart(2, "0")}-${String(x.getDate()).padStart(2, "0")}`);
    }
    return out;
  });
  const inRange = $derived(new Set(dayKeys));

  function costOf(r: Row): number | null {
    const p = app.pricing[r.model];
    if (!p?.input || !p.output) return null;
    return ((r.input - r.cached) * p.input + r.cached * (p.cached ?? p.input) + r.output * p.output) / 1e6;
  }
  function savingsOf(r: Row): number {
    const p = app.pricing[r.model];
    if (!p?.input) return 0;
    return (r.cached * (p.input - (p.cached ?? p.input))) / 1e6;
  }

  type Agg = { cost: number; tokens: number; input: number; cached: number; output: number; reasoning: number; requests: number; pricedTokens: number; savings: number };
  const empty = (): Agg => ({ cost: 0, tokens: 0, input: 0, cached: 0, output: 0, reasoning: 0, requests: 0, pricedTokens: 0, savings: 0 });
  function addRow(a: Agg, r: Row) {
    const c = costOf(r);
    const t = r.input + r.output;
    a.cost += c ?? 0;
    a.tokens += t;
    a.input += r.input;
    a.cached += r.cached;
    a.output += r.output;
    a.reasoning += r.reasoning;
    a.requests += r.requests;
    if (c != null) a.pricedTokens += t;
    a.savings += savingsOf(r);
  }

  const data = $derived.by(() => {
    const total = empty();
    const bySource = new Map<string, Agg>();
    const byModel = new Map<string, Agg>();
    const byDay = new Map<string, Map<string, Agg>>();
    const dayTotals = new Map<string, Agg>();
    for (const r of rows) {
      const day = localDay(r.hour);
      if (!inRange.has(day)) continue;
      addRow(total, r);
      for (const [map, key] of [[bySource, r.source], [byModel, r.model], [dayTotals, day]] as const) {
        if (!map.has(key)) map.set(key, empty());
        addRow(map.get(key)!, r);
      }
      if (!byDay.has(day)) byDay.set(day, new Map());
      const m = byDay.get(day)!;
      if (!m.has(r.source)) m.set(r.source, empty());
      addRow(m.get(r.source)!, r);
    }
    return { total, bySource, byModel, byDay, dayTotals };
  });

  // Stable colour per entity: Codex first, then accounts in their own order.
  const sources = $derived.by(() => {
    const order = ["codex", ...app.accounts.map((a) => a.id)];
    const present = [...data.bySource.keys()];
    for (const s of present) if (!order.includes(s)) order.push(s);
    return order.map((id, i) => ({ id, color: SERIES[i % SERIES.length] })).filter((s) => present.includes(s.id));
  });
  const sourceName = (id: string) => (id === "codex" ? "Codex CLI & apps" : `hrns · ${app.accountLabel(id)}`);

  const activeDays = $derived([...data.dayTotals.values()].filter((d) => d.tokens > 0).length);
  const val = (a: Agg | undefined) => (a ? (metric === "cost" ? a.cost : a.tokens) : 0);

  // ---- chart ----
  let chartW = $state(600);
  const CH = 240, PAD_L = 56, PAD_B = 26, PAD_T = 8;
  const maxY = $derived(Math.max(...dayKeys.map((d) => val(data.dayTotals.get(d))), metric === "cost" ? 0.01 : 1));
  const niceMax = $derived.by(() => {
    const raw = maxY;
    const p = 10 ** Math.floor(Math.log10(raw));
    for (const m of [1, 2, 2.5, 5, 10]) if (m * p >= raw) return m * p;
    return 10 * p;
  });
  const plotW = $derived(Math.max(100, chartW - PAD_L - 8));
  const slot = $derived(plotW / dayKeys.length);
  const barW = $derived(Math.max(2, Math.min(28, slot - (days > 30 ? 2 : 4))));
  const y = (v: number) => PAD_T + (CH - PAD_T - PAD_B) * (1 - v / niceMax);

  let hover = $state<string | null>(null);

  // ---- formatting ----
  const usd = (n: number) =>
    n >= 1000 ? `$${n.toLocaleString("en-US", { maximumFractionDigits: 0 })}` : `$${n.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
  const tok = (n: number) =>
    n >= 1e9 ? `${(n / 1e9).toFixed(n >= 1e10 ? 1 : 2)}B` : n >= 1e6 ? `${(n / 1e6).toFixed(n >= 1e7 ? 1 : 2)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(n >= 1e4 ? 0 : 1)}k` : String(n);
  const axis = (n: number) => (metric === "cost" ? (n >= 1000 ? `$${(n / 1000).toFixed(n % 1000 ? 1 : 0)}k` : `$${n < 10 && n % 1 ? n.toFixed(2) : n}`) : tok(n));
  const pct = (a: number, b: number) => (b > 0 ? `${((a / b) * 100).toFixed(1)}%` : "0%");
  const fmtDay = (k: string, long = false) =>
    new Date(`${k}T12:00:00`).toLocaleDateString(undefined, long ? { weekday: "short", month: "short", day: "numeric" } : { month: "short", day: "numeric" });

  const rangeLabel = $derived(`${fmtDay(dayKeys[0])} to ${fmtDay(dayKeys[dayKeys.length - 1])}`);

  const tableRows = $derived.by(() => {
    if (breakdown === "model") {
      return [...data.byModel.entries()]
        .map(([k, a]) => ({ key: k, label: k, a, priced: costOf({ model: k, input: 1, cached: 0, output: 1 } as Row) != null }))
        .sort((x, y) => y.a.cost - x.a.cost || y.a.tokens - x.a.tokens);
    }
    return [...data.dayTotals.entries()]
      .filter(([, a]) => a.tokens > 0)
      .map(([k, a]) => ({ key: k, label: fmtDay(k, true), a, priced: true }))
      .sort((x, y) => (x.key < y.key ? 1 : -1));
  });
</script>

<div class="usage">
  <div class="inner">
    <div class="top">
      <div>
        <h1>Usage</h1>
        <div class="sub">{rangeLabel}</div>
      </div>
      <div class="controls">
        <div class="seg">
          {#each RANGES as r}
            <button class:sel={days === r} onclick={() => (days = r)}>{r} days</button>
          {/each}
        </div>
        <button class="refresh" class:spin={loading} title="Rescan" onclick={load} aria-label="Rescan usage">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-2.6-6.4L21 8" /><path d="M21 3v5h-5" /></svg>
        </button>
      </div>
    </div>

    {#if error}<div class="err">{error}</div>{/if}

    <div class="hero-grid">
      <section class="hero">
        <div class="eyebrow">Raw token cost</div>
        <div class="big">{usd(data.total.cost)}<span class="star">*</span></div>
        <div class="foot">* if billed at full API rate · covered by your ChatGPT plan</div>

        <div class="sources">
          {#each sources as s (s.id)}
            {@const a = data.bySource.get(s.id)!}
            <div class="src">
              <div class="src-top">
                <span class="key" style="background:{s.color}"></span>
                <span class="src-name">{sourceName(s.id)}</span>
                <span class="src-val">{usd(a.cost)}</span>
              </div>
              <div class="track"><div style="width:{data.total.cost ? (a.cost / data.total.cost) * 100 : 0}%;background:{s.color}"></div></div>
              <div class="src-sub">{pct(a.cost, data.total.cost)} of cost · {tok(a.tokens)} tokens</div>
            </div>
          {:else}
            <div class="src-sub">{loading ? "Scanning Codex sessions…" : "No usage in this range yet."}</div>
          {/each}
        </div>
      </section>

      <section class="chart-card">
        <div class="chart-head">
          <h2>Daily {metric}</h2>
          <div class="seg small">
            <button class:sel={metric === "cost"} onclick={() => (metric = "cost")}>Cost</button>
            <button class:sel={metric === "tokens"} onclick={() => (metric = "tokens")}>Tokens</button>
          </div>
        </div>
        {#if sources.length > 1}
          <div class="legend">
            {#each sources as s (s.id)}<span><i style="background:{s.color}"></i>{sourceName(s.id)}</span>{/each}
          </div>
        {/if}

        <div class="chart" bind:clientWidth={chartW}>
          <svg width={chartW} height={CH} role="img" aria-label="Daily {metric} by source">
            {#each [0, 0.5, 1] as f}
              <line class="grid" x1={PAD_L} x2={chartW - 8} y1={y(niceMax * f)} y2={y(niceMax * f)} />
              <text class="ylab" x={PAD_L - 8} y={y(niceMax * f) + 4} text-anchor="end">{axis(niceMax * f)}</text>
            {/each}

            {#each dayKeys as d, i (d)}
              {@const x = PAD_L + i * slot + (slot - barW) / 2}
              {@const stack = data.byDay.get(d)}
              <g class="bar" class:dim={hover && hover !== d}>
                {#if stack}
                  {@const segs = sources.filter((s) => val(stack.get(s.id)) > 0)}
                  {#each segs as s, j (s.id)}
                    {@const below = segs.slice(0, j).reduce((acc, t) => acc + val(stack.get(t.id)), 0)}
                    {@const v = val(stack.get(s.id))}
                    {@const top = y(below + v)}
                    {@const h = Math.max(1, y(below) - top - (j > 0 ? 2 : 0))}
                    <rect x={x} y={top} width={barW} height={h} rx={j === segs.length - 1 ? Math.min(4, barW / 2) : 0} fill={s.color} />
                  {/each}
                {/if}
              </g>
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <rect class="hit" x={PAD_L + i * slot} y={PAD_T} width={slot} height={CH - PAD_T - PAD_B} onpointerenter={() => (hover = d)} onpointerleave={() => (hover = null)} />
            {/each}

            <line class="base" x1={PAD_L} x2={chartW - 8} y1={y(0)} y2={y(0)} />
            {#each [0, Math.floor((dayKeys.length - 1) / 2), dayKeys.length - 1] as i}
              <text class="xlab" x={PAD_L + i * slot + slot / 2} y={CH - 6} text-anchor={i === 0 ? "start" : i === dayKeys.length - 1 ? "end" : "middle"}>{fmtDay(dayKeys[i]).toUpperCase()}</text>
            {/each}
          </svg>

          {#if hover}
            {@const i = dayKeys.indexOf(hover)}
            {@const stack = data.byDay.get(hover)}
            {@const left = PAD_L + i * slot + slot / 2}
            <div class="tip" style="left:{Math.min(Math.max(left, 110), chartW - 110)}px">
              <div class="tip-day">{fmtDay(hover, true)}</div>
              {#each sources as s (s.id)}
                {@const a = stack?.get(s.id)}
                {#if a}
                  <div class="tip-row"><i style="background:{s.color}"></i><b>{metric === "cost" ? usd(a.cost) : tok(a.tokens)}</b><span>{sourceName(s.id)}</span></div>
                {/if}
              {/each}
              {#if !stack}<div class="tip-row"><span>No usage</span></div>{/if}
            </div>
          {/if}
        </div>
      </section>
    </div>

    <div class="stats">
      <div class="stat">
        <div class="s-l">Processed tokens</div>
        <div class="s-v">{tok(data.total.tokens)}</div>
        <div class="s-h">{activeDays ? `${tok(Math.round(data.total.tokens / activeDays))} per active day` : "—"}</div>
      </div>
      <div class="stat">
        <div class="s-l">Cached input</div>
        <div class="s-v">{tok(data.total.cached)}</div>
        <div class="s-h">{pct(data.total.cached, data.total.input)} of input</div>
      </div>
      <div class="stat">
        <div class="s-l">Uncached input</div>
        <div class="s-v">{tok(data.total.input - data.total.cached)}</div>
        <div class="s-h">{data.total.requests.toLocaleString()} requests</div>
      </div>
      <div class="stat">
        <div class="s-l">Output</div>
        <div class="s-v">{tok(data.total.output)}</div>
        <div class="s-h">includes {tok(data.total.reasoning)} reasoning</div>
      </div>
      <div class="stat">
        <div class="s-l">Cache savings</div>
        <div class="s-v">{usd(data.total.savings)}</div>
        <div class="s-h">{data.total.cost ? `${(data.total.savings / data.total.cost).toFixed(1)}× the raw token cost` : "—"}</div>
      </div>
    </div>

    <div class="bottom">
      <section class="table">
        <div class="chart-head">
          <h2>Breakdown</h2>
          <div class="seg small">
            <button class:sel={breakdown === "model"} onclick={() => (breakdown = "model")}>Model</button>
            <button class:sel={breakdown === "day"} onclick={() => (breakdown = "day")}>Day</button>
          </div>
        </div>
        <table>
          <thead>
            <tr><th>{breakdown === "model" ? "Model" : "Day"}</th><th>Cost</th><th>Share</th><th>Tokens</th></tr>
          </thead>
          <tbody>
            {#each tableRows as r (r.key)}
              <tr>
                <td class="name">{r.label}{#if !r.priced}<span class="unpriced">unpriced</span>{/if}</td>
                <td>{r.priced ? usd(r.a.cost) : "—"}</td>
                <td class="muted">{pct(r.a.cost, data.total.cost)}</td>
                <td>{tok(r.a.tokens)}</td>
              </tr>
            {:else}
              <tr><td colspan="4" class="muted">Nothing to show for this range.</td></tr>
            {/each}
          </tbody>
        </table>
      </section>

      <section class="quality">
        <h2>Cost quality</h2>
        <div class="kv"><span>Model priced</span><b>{pct(data.total.pricedTokens, data.total.tokens)}</b></div>
        <div class="kv"><span>Unpriced</span><b>{pct(data.total.tokens - data.total.pricedTokens, data.total.tokens)}</b></div>
        <div class="kv"><span>Requests</span><b>{data.total.requests.toLocaleString()}</b></div>
        <div class="kv"><span>Cache savings</span><b>{usd(data.total.savings)}</b></div>
        <p class="note">Prices from models.dev. Codex CLI, IDE and app sessions are read from <code>~/.codex/sessions</code>; hrns chats are logged as they finish.</p>
      </section>
    </div>
  </div>
</div>

<style>
  .usage { flex: 1; min-height: 0; overflow-y: auto; }
  .inner { max-width: 1180px; margin: 0 auto; padding: 4px 40px 56px; }
  .top { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; margin-bottom: 28px; }
  h1 { font-size: 26px; font-weight: 650; margin: 0; letter-spacing: -0.01em; }
  h2 { font-size: 14px; font-weight: 600; margin: 0; }
  .sub { color: var(--muted); font-size: 13.5px; margin-top: 4px; }
  .controls { display: flex; gap: 8px; align-items: center; }
  .seg { display: flex; padding: 3px; border-radius: 10px; background: var(--panel); border: 1px solid var(--border); }
  .seg button { height: 28px; padding: 0 12px; border-radius: 7px; font-size: 12.5px; color: var(--muted); }
  .seg button:hover { color: var(--text); }
  .seg button.sel { background: var(--hover); color: var(--text); }
  .seg.small button { height: 24px; padding: 0 10px; font-size: 11px; letter-spacing: 0.04em; text-transform: uppercase; }
  .refresh { width: 36px; height: 36px; border-radius: 10px; border: 1px solid var(--border); display: grid; place-items: center; color: var(--muted); background: var(--panel); }
  .refresh:hover { color: var(--text); }
  .refresh.spin svg { animation: rot 0.9s linear infinite; }
  @keyframes rot { to { rotate: 360deg; } }
  .err { color: var(--danger); margin-bottom: 16px; font-size: 13px; }

  .hero-grid { display: grid; grid-template-columns: minmax(260px, 380px) 1fr; gap: 40px; }
  @media (max-width: 1000px) { .hero-grid { grid-template-columns: 1fr; } }
  .eyebrow { font-size: 11.5px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted); }
  .big { font-size: 44px; font-weight: 700; letter-spacing: -0.02em; margin-top: 4px; font-variant-numeric: tabular-nums; }
  .star { color: var(--muted); font-weight: 500; }
  .foot { color: var(--muted); font-size: 12.5px; }
  .sources { margin-top: 28px; display: grid; gap: 20px; }
  .src-top { display: flex; align-items: center; gap: 10px; font-size: 14px; }
  .key { width: 10px; height: 10px; border-radius: 3px; flex: none; }
  .src-name { flex: 1; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .src-val { font-variant-numeric: tabular-nums; font-weight: 550; }
  .track { height: 3px; border-radius: 3px; background: var(--raised); margin: 8px 0 6px; overflow: hidden; }
  .track > div { height: 100%; border-radius: 3px; }
  .src-sub { color: var(--muted); font-size: 12.5px; }

  .chart-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 12px; }
  .chart-card { min-width: 0; }
  .legend { display: flex; flex-wrap: wrap; gap: 14px; font-size: 12px; color: var(--muted); margin: -4px 0 8px; }
  .legend i, .tip-row i { display: inline-block; width: 10px; height: 3px; border-radius: 2px; margin-right: 6px; vertical-align: middle; }
  .chart { position: relative; width: 100%; }
  svg { display: block; overflow: visible; }
  .grid { stroke: var(--border); stroke-dasharray: 2 4; }
  .base { stroke: var(--border); }
  .ylab, .xlab { fill: var(--faint); font-size: 10.5px; font-variant-numeric: tabular-nums; }
  .xlab { letter-spacing: 0.04em; }
  .bar { transition: opacity 0.12s; }
  .bar.dim { opacity: 0.45; }
  .hit { fill: transparent; }
  .tip {
    position: absolute; top: 0; translate: -50% 0; pointer-events: none; z-index: 2; min-width: 180px;
    background: var(--panel); border: 1px solid var(--border); border-radius: 10px; padding: 8px 10px;
    box-shadow: 0 12px 30px rgb(0 0 0 / 0.5); font-size: 12px;
  }
  .tip-day { color: var(--muted); margin-bottom: 4px; }
  .tip-row { display: flex; align-items: center; gap: 6px; padding: 1px 0; }
  .tip-row b { font-variant-numeric: tabular-nums; font-weight: 600; }
  .tip-row span { color: var(--muted); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }

  .stats {
    margin: 36px 0; display: grid; grid-template-columns: repeat(5, 1fr);
    border: 1px solid var(--border); border-radius: 12px; background: var(--panel); overflow: hidden;
  }
  @media (max-width: 1000px) { .stats { grid-template-columns: repeat(2, 1fr); } }
  .stat { padding: 16px 18px; border-right: 1px solid var(--border); }
  .stat:last-child { border-right: 0; }
  .s-l { font-size: 12.5px; color: var(--muted); }
  .s-v { font-size: 22px; font-weight: 600; margin: 6px 0 4px; font-variant-numeric: tabular-nums; }
  .s-h { font-size: 12px; color: var(--faint); }

  .bottom { display: grid; grid-template-columns: 1fr 320px; gap: 40px; }
  @media (max-width: 1000px) { .bottom { grid-template-columns: 1fr; } }
  table { width: 100%; border-collapse: collapse; font-size: 13.5px; }
  th { text-align: right; font-weight: 500; color: var(--muted); font-size: 12.5px; padding: 8px 0; border-bottom: 1px solid var(--border); }
  th:first-child, td:first-child { text-align: left; }
  td { text-align: right; padding: 11px 0; border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent); font-variant-numeric: tabular-nums; }
  td.name { font-family: var(--mono); font-size: 13px; }
  .muted { color: var(--muted); }
  .unpriced { margin-left: 8px; font: 10.5px var(--font); color: var(--faint); background: var(--raised); padding: 1px 6px; border-radius: 4px; }
  .quality h2 { margin-bottom: 14px; }
  .kv { display: flex; justify-content: space-between; padding: 9px 0; border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent); font-size: 13.5px; color: var(--muted); }
  .kv b { color: var(--text); font-weight: 550; font-variant-numeric: tabular-nums; }
  .note { margin-top: 14px; font-size: 12px; color: var(--faint); line-height: 1.5; }
  .note code { font: 11.5px var(--mono); }
</style>
