<script lang="ts">
  import { app, projectOf, type Summary } from "./store.svelte";
  import { drag, startDrag } from "./drag.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import AnimatedTitle from "./AnimatedTitle.svelte";

  let query = $state("");
  let search: HTMLInputElement;

  const name = (dir: string | null | undefined) => (dir ? dir.split("/").filter(Boolean).pop() ?? dir : "No project");

  function ago(ts: number) {
    const m = Math.round((Date.now() - ts) / 60_000);
    if (m < 1) return "now";
    if (m < 60) return `${m}m`;
    if (m < 1440) return `${Math.round(m / 60)}h`;
    if (m < 1440 * 7) return `${Math.round(m / 1440)}d`;
    return new Date(ts).toLocaleDateString(undefined, { month: "short", day: "numeric" });
  }

  // Plain-text preview: drop markdown syntax and collapse whitespace.
  const plain = (s: string) =>
    s.replace(/```[\s\S]*?(```|$)/g, " [code] ")
      .replace(/[#*_>`~|]/g, "")
      .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
      .replace(/\s+/g, " ")
      .trim();

  const shortModel = (id: string) => app.modelName(id).replace(/^GPT-/i, "");

  type Status = "running" | "waiting" | "failed" | "idle";
  const status = (t: Summary): Status =>
    app.waiting[t.id] ? "waiting" : app.streaming[t.id] ? "running" : t.failed ? "failed" : "idle";

  // Codex-style: chats grouped by project folder, most recently active project first.
  const projects = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const map = new Map<string, Summary[]>();
    if (app.folder && !q) map.set(app.folder, []);
    for (const t of app.threads) {
      if (q && !t.title.toLowerCase().includes(q) && !t.preview.toLowerCase().includes(q)) continue;
      // Chats in a worktree belong to their project.
      const key = projectOf(t) ?? "";
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(t);
    }
    return [...map.entries()]
      .map(([dir, items]) => ({ dir, items }))
      .sort((a, b) => (b.items[0]?.updatedAt ?? Infinity) - (a.items[0]?.updatedAt ?? Infinity));
  });

  let collapsed = $state<Record<string, boolean>>({});
  let accountMenu = $state(false);

  // Usage details popover (fixed-positioned so the scrolling list can't clip it).
  let stats = $state<{ id: string; x: number; y: number; up: boolean } | null>(null);
  const statsFor = $derived(stats ? app.threads.find((t) => t.id === stats!.id) : undefined);

  function showStats(e: MouseEvent, id: string) {
    e.stopPropagation();
    if (stats?.id === id) return (stats = null);
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const up = r.bottom + 330 > window.innerHeight;
    stats = { id, x: r.left, y: up ? window.innerHeight - r.top + 6 : r.bottom + 6, up };
  }

  const fmtTokens = (n: number) =>
    n < 1000 ? String(n) : n < 1e6 ? `${(n / 1e3).toFixed(n < 1e4 ? 1 : 0)}k` : `${(n / 1e6).toFixed(n < 1e7 ? 2 : 1)}M`;
  const fmtCost = (c: number | null) => (c == null ? "—" : c === 0 ? "$0" : c < 0.01 ? "<$0.01" : `$${c.toFixed(c < 10 ? 2 : 1)}`);

  async function addProject() {
    const dir = await app.pickFolder();
    if (dir) app.newChatIn(dir);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") { stats = null; accountMenu = false; }
    if ((e.metaKey || e.ctrlKey) && e.key === "k") {
      e.preventDefault();
      search?.focus();
    }
  }

  const p5 = $derived(app.rateLimits?.primary);
  const pw = $derived(app.rateLimits?.secondary);

  function pct(w: { usedPercent: number } | null | undefined) {
    return Math.max(0, Math.min(100, Math.round(w?.usedPercent ?? 0)));
  }
  function windowLabel(mins: number | undefined, fallback: string) {
    if (!mins) return fallback;
    if (mins >= 1440 * 7) return "Week";
    if (mins >= 60) return `${Math.round(mins / 60)}h`;
    return `${mins}m`;
  }
</script>

<svelte:window {onkeydown} onclick={() => { stats = null; accountMenu = false; }} onresize={() => (stats = null)} />

<aside class="sidebar">
  <div class="top" data-tauri-drag-region>
    <span class="brand" data-tauri-drag-region>hrns</span>
  </div>

  <div class="actions">
    <button class="new" onclick={() => app.newChat()}>
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" /></svg>
      New chat
      <kbd>⌘N</kbd>
    </button>
    <button class="square" title="Add project folder" onclick={addProject}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" /><path d="M12 11v5M9.5 13.5h5" stroke-linecap="round" /></svg>
    </button>
  </div>

  <label class="search">
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></svg>
    <input bind:this={search} placeholder="Search chats" bind:value={query} spellcheck="false" />
    {#if query}
      <button class="clear" title="Clear" onclick={() => (query = "")}>×</button>
    {:else}
      <kbd>⌘K</kbd>
    {/if}
  </label>

  <nav class="list" onscroll={() => (stats = null)}>
    {#each projects as p (p.dir)}
      <div class="project">
        <button class="phead" title={p.dir || "No project"} onclick={() => (collapsed[p.dir] = !collapsed[p.dir])}>
          <svg class="chev" class:closed={collapsed[p.dir]} width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.5" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
          {#if p.dir}<ProjectIcon path={p.dir} size={16} />{/if}
          <span class="pname">{name(p.dir)}</span>
          <span class="count">{p.items.length}</span>
        </button>
        {#if p.dir}
          <button class="padd" title="New chat in {name(p.dir)}" onclick={() => app.newChatIn(p.dir)}>
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
          </button>
        {/if}
      </div>

      {#if !collapsed[p.dir]}
        {#each p.items as t (t.id)}
          {@const st = status(t)}
          {@const u = app.usageOf(t)}
          <div class="card" class:active={app.isOpen(t.id)} class:lifted={drag.id === t.id}>
            <button class="open" title="Drag into the chat to open side by side" onclick={() => app.open(t.id)} onpointerdown={(e) => startDrag(e, t.id, t.title)}>
              <div class="row1">
                <span class="status {st}" title={st === "waiting" ? "Needs your approval" : st === "running" ? "Working" : st === "failed" ? "Last turn failed" : ""}>
                  {#if st === "waiting"}
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><path d="M12 8v5M12 16.5v.5" /><circle cx="12" cy="12" r="9.5" /></svg>
                  {:else if st === "failed"}
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><circle cx="12" cy="12" r="9.5" /><path d="m9 9 6 6M15 9l-6 6" /></svg>
                  {:else if st === "running"}
                    <span class="spin"></span>
                  {:else}
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M21 12a8 8 0 0 1-11.6 7.1L4 20l1-4.6A8 8 0 1 1 21 12Z" /></svg>
                  {/if}
                </span>
                <span class="title"><AnimatedTitle text={t.title} id={t.id} /></span>
                <span class="ago">{ago(t.updatedAt)}</span>
              </div>

              {#if st === "waiting"}
                <div class="preview warn">Waiting for your approval</div>
              {:else if t.preview}
                <div class="preview">{plain(t.preview)}</div>
              {/if}

            </button>
            <button class="meta" class:on={stats?.id === t.id} title="Usage details" onclick={(e) => showStats(e, t.id)}>
              <span class="chip model">{shortModel(t.model)}</span>
              {#if t.worktree}
                <span class="chip branch" title="Worktree on {t.worktree.branch} (from {t.worktree.base})">
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><circle cx="6" cy="5" r="2" /><circle cx="6" cy="19" r="2" /><circle cx="18" cy="7" r="2" /><path d="M6 7v10M18 9c0 4-6 3-11.5 8" /></svg>
                  <span class="bname">{t.worktree.branch.replace(/^hrns\//, "")}</span>
                </span>
              {/if}
              {#if t.files}
                <span class="chip diff"><span class="add">+{t.additions}</span><span class="del-n">−{t.deletions}</span></span>
              {/if}
              {#if u.tokens}
                <span class="chip">
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2 3 14h9l-1 8 10-12h-9Z" /></svg>
                  {fmtTokens(u.tokens)}
                </span>
                <span class="chip cost">{fmtCost(u.cost)}</span>
              {/if}
            </button>
            <button class="del" title="Delete chat" onclick={() => app.remove(t.id)}>
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3" /></svg>
            </button>
          </div>
        {:else}
          <button class="none" onclick={() => app.newChatIn(p.dir)}>Start a chat in {name(p.dir)}</button>
        {/each}
      {/if}
    {:else}
      <div class="empty">
        {#if query}No chats match “{query}”{:else}
          <p>No projects yet</p>
          <button class="cta" onclick={addProject}>Add a project folder</button>
        {/if}
      </div>
    {/each}
  </nav>

  {#if app.account}
    <footer>
      {#if accountMenu}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="acct-menu" onclick={(e) => e.stopPropagation()}>
          <div class="pop-sec first">Accounts</div>
          {#each app.accounts as a (a.id)}
            {@const st = app.statuses[a.id]}
            {@const p5 = st?.rateLimits?.primary}
            <button class="acct" class:sel={a.id === app.activeAccount} onclick={() => { app.setActiveAccount(a.id); accountMenu = false; }}>
              <div class="avatar sm" class:off={!a.email}>{(a.email ?? "?")[0].toUpperCase()}</div>
              <div class="who-meta">
                <div class="email">{a.email ?? (app.pendingLogin === a.id ? "Waiting for sign-in…" : "Not signed in")}</div>
                <div class="plan">{a.plan ? `ChatGPT ${a.plan}` : a.id === "default" ? "Codex CLI login" : ""}{#if p5} · {pct(p5)}% of 5h{/if}</div>
              </div>
              {#if a.id === app.activeAccount}
                <svg class="check" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><path d="m5 12 5 5 9-10" /></svg>
              {/if}
            </button>
          {/each}
          <div class="sep"></div>
          <button class="acct action" onclick={() => { app.addAccount(); }}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
            Add ChatGPT account
          </button>
          <button class="acct action" onclick={() => { accountMenu = false; app.openSettings("account"); }}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21a8 8 0 0 0-16 0M12 13a5 5 0 1 0 0-10 5 5 0 0 0 0 10Z" /></svg>
            Manage accounts
          </button>
        </div>
      {/if}

      <div class="who">
        <button class="who-btn" class:on={accountMenu} title="{app.account?.email ?? 'Account'} · ChatGPT {app.account?.planType ?? ''}" onclick={(e) => { e.stopPropagation(); accountMenu = !accountMenu; if (accountMenu) app.accounts.forEach((a) => a.email && !app.statuses[a.id] && app.refreshStatus(a.id)); }}>
          <div class="avatar">{(app.account?.email ?? "?")[0].toUpperCase()}</div>
          <span class="email">{app.account?.email?.split("@")[0] ?? "Signed in"}</span>
          <svg class="updown" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="m7 15 5 5 5-5M7 9l5-5 5 5" /></svg>
        </button>

        <!-- Usage at a glance: outer ring = short window, inner ring = weekly. Opens the Usage page. -->
        <button
          class="meter"
          class:on={app.view === "usage"}
          class:hot={pct(p5) > 80 || pct(pw) > 80}
          title={[p5 && `${windowLabel(p5.windowDurationMins, "5h")}: ${pct(p5)}% used`, pw && `${windowLabel(pw.windowDurationMins, "Week")}: ${pct(pw)}% used`].filter(Boolean).join(" · ") || "Usage"}
          onclick={() => (app.view = app.view === "usage" ? "chat" : "usage")}
        >
          <svg width="18" height="18" viewBox="0 0 36 36" aria-hidden="true">
            <circle class="track" cx="18" cy="18" r="15" />
            <circle class="fill" cx="18" cy="18" r="15" pathLength="100" stroke-dasharray="{pct(p5)} 100" />
            <circle class="track" cx="18" cy="18" r="9.5" />
            <circle class="fill week" cx="18" cy="18" r="9.5" pathLength="100" stroke-dasharray="{pct(pw)} 100" />
          </svg>
          {#if p5}<span>{pct(p5)}%</span>{/if}
        </button>

        <button class="icon-btn" class:on={app.view === "settings"} title="Settings (⌘,)" onclick={() => app.openSettings()}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" /></svg>
        </button>
      </div>
    </footer>
  {/if}
  {#if stats && statsFor}
    {@const t = statsFor}
    {@const u = app.usageOf(t)}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="pop" style="left:{stats.x}px;{stats.up ? 'bottom' : 'top'}:{stats.y}px" onclick={(e) => e.stopPropagation()}>
      <div class="pop-title">{t.title}</div>
      <div class="pop-sub">{app.modelName(t.model)}</div>

      <div class="pop-sec">Changes</div>
      <div class="kv"><span>Files changed</span><b>{t.files}</b></div>
      <div class="kv"><span>Lines</span><b><span class="add">+{t.additions}</span> <span class="del-n">−{t.deletions}</span></b></div>
      <div class="kv"><span>Commands run</span><b>{t.commands}</b></div>
      <div class="kv"><span>Messages</span><b>{t.messages}</b></div>

      <div class="pop-sec">Tokens</div>
      <div class="kv"><span>Input</span><b>{fmtTokens(u.input - u.cached)}</b></div>
      <div class="kv"><span>Cached input</span><b>{fmtTokens(u.cached)}</b></div>
      <div class="kv"><span>Output</span><b>{fmtTokens(u.output)}</b></div>
      <div class="kv sub"><span>incl. reasoning</span><b>{fmtTokens(u.reasoning)}</b></div>
      <div class="kv total"><span>Total</span><b>{fmtTokens(u.tokens)}</b></div>

      <div class="kv cost-row"><span>API-equivalent cost</span><b>{fmtCost(u.cost)}</b></div>
      <div class="note">Covered by your ChatGPT plan. Estimated from public API prices.</div>
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    width: 288px; flex: none; height: 100%;
    display: flex; flex-direction: column;
    background: var(--sidebar); border-right: 1px solid var(--border);
  }
  .top { height: 52px; flex: none; display: flex; align-items: center; justify-content: flex-end; padding: 0 16px 0 160px; }
  .brand { font-weight: 650; font-size: 14px; letter-spacing: -0.01em; color: var(--muted); }

  .actions { display: flex; gap: 6px; padding: 4px 10px 10px; }
  .new {
    flex: 1; height: 36px; padding: 0 12px; border-radius: 10px;
    display: flex; align-items: center; gap: 9px; font-weight: 550; font-size: 13.5px;
    background: var(--raised); border: 1px solid var(--border);
  }
  .new:hover { background: var(--hover); }
  .square {
    width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; color: var(--muted);
    border: 1px solid var(--border);
  }
  .square:hover { background: var(--hover); color: var(--text); }
  kbd { margin-left: auto; font: 11px var(--font); color: var(--faint); }

  .search {
    margin: 0 10px 8px; height: 32px; padding: 0 10px; border-radius: 9px;
    display: flex; align-items: center; gap: 8px; color: var(--faint);
    background: color-mix(in srgb, var(--raised) 55%, transparent);
  }
  .search:focus-within { background: var(--raised); color: var(--muted); }
  .search input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font-size: 13px; }
  .search input::placeholder { color: var(--faint); }
  .clear { font-size: 16px; line-height: 1; color: var(--faint); padding: 0 2px; }
  .clear:hover { color: var(--text); }

  .list { flex: 1; overflow-y: auto; padding: 0 8px 14px; }

  .project { position: sticky; top: 0; z-index: 1; display: flex; align-items: center; margin-top: 8px; background: var(--sidebar); }
  .phead {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; padding: 8px 6px 6px;
    font-size: 11px; font-weight: 650; letter-spacing: 0.06em; text-transform: uppercase; color: var(--faint);
  }
  .phead:hover { color: var(--muted); }
  .phead :global(.pi) { margin: -2px 1px -2px 0; }
  .pname { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .count {
    font-size: 10.5px; font-weight: 600; letter-spacing: 0; padding: 0 6px; border-radius: 999px;
    background: var(--raised); color: var(--faint); line-height: 16px;
  }
  .chev { flex: none; transition: rotate 0.12s; }
  .chev.closed { rotate: -90deg; }
  .padd { width: 22px; height: 22px; margin-right: 4px; border-radius: 6px; display: grid; place-items: center; color: var(--faint); opacity: 0; }
  .project:hover .padd { opacity: 1; }
  .padd:hover { background: var(--raised); color: var(--text); }

  .card { position: relative; border-radius: 10px; margin-bottom: 2px; border: 1px solid transparent; }
  .card:hover { background: var(--hover); }
  .card.active { background: var(--raised); border-color: var(--border); }
  .card.lifted { opacity: 0.45; }
  .card.active::before {
    content: ""; position: absolute; left: -1px; top: 10px; bottom: 10px; width: 2px; border-radius: 2px; background: var(--accent);
  }
  .open { width: 100%; text-align: left; padding: 9px 10px 9px 10px; display: grid; gap: 4px; }

  .row1 { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .status { flex: none; width: 14px; height: 14px; display: grid; place-items: center; color: var(--faint); }
  .status.waiting { color: #fbbf24; }
  .status.failed { color: var(--danger); }
  .status.running { color: var(--accent); }
  .spin {
    width: 10px; height: 10px; border-radius: 50%;
    border: 1.6px solid color-mix(in srgb, var(--accent) 30%, transparent); border-top-color: var(--accent);
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin { to { rotate: 360deg; } }
  .title { flex: 1; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 13.5px; font-weight: 550; }
  .card:not(.active) .title { color: color-mix(in srgb, var(--text) 88%, transparent); }
  .ago { flex: none; font-size: 11px; color: var(--faint); font-variant-numeric: tabular-nums; }
  .card:hover .ago { visibility: hidden; }

  .preview {
    padding-left: 22px; font-size: 12.5px; line-height: 1.45; color: var(--muted);
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
  .preview.warn { color: #fbbf24; }

  .meta {
    display: flex; align-items: center; gap: 4px; flex-wrap: wrap;
    margin: -4px 0 0 30px; padding: 2px 2px 9px; border-radius: 6px; max-width: calc(100% - 40px);
  }
  .meta:hover .chip, .meta.on .chip { color: var(--muted); }
  .chip.diff { gap: 5px; }
  .add { color: #4ade80; }
  .del-n { color: var(--danger); }
  .chip.cost { color: color-mix(in srgb, #4ade80 70%, var(--muted)); }

  .pop {
    position: fixed; z-index: 100; width: 260px; padding: 12px 14px;
    background: var(--panel); border: 1px solid var(--border); border-radius: 12px;
    box-shadow: 0 18px 50px rgb(0 0 0 / 0.55); font-size: 12.5px; user-select: text; -webkit-user-select: text;
  }
  .pop-title { font-weight: 600; font-size: 13px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .pop-sub { color: var(--faint); font-size: 11.5px; margin-top: 1px; }
  .pop-sec { margin: 12px 0 4px; font-size: 10.5px; font-weight: 650; letter-spacing: 0.06em; text-transform: uppercase; color: var(--faint); }
  .kv { display: flex; justify-content: space-between; gap: 10px; padding: 2px 0; color: var(--muted); }
  .kv b { font-weight: 550; color: var(--text); font-variant-numeric: tabular-nums; }
  .kv.sub { padding-left: 10px; font-size: 11.5px; }
  .kv.sub b { color: var(--muted); font-weight: 500; }
  .kv.total { border-top: 1px solid var(--border); margin-top: 4px; padding-top: 6px; }
  .cost-row { margin-top: 10px; padding: 8px 10px; border-radius: 8px; background: var(--raised); }
  .cost-row b { color: #4ade80; }
  .note { margin-top: 6px; font-size: 11px; color: var(--faint); line-height: 1.4; }
  .chip {
    display: inline-flex; align-items: center; gap: 4px; height: 18px; padding: 0 6px; border-radius: 5px;
    font-size: 10.5px; font-weight: 550; color: var(--faint); background: color-mix(in srgb, var(--raised) 70%, transparent);
    font-variant-numeric: tabular-nums;
  }
  .card.active .chip { background: var(--hover); }
  .chip.model { color: color-mix(in srgb, var(--accent) 75%, var(--muted)); }
  .chip.branch { gap: 4px; min-width: 0; flex: 0 1 auto; }
  .chip.branch .bname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 110px; font-family: var(--mono); font-size: 0.95em; }

  .del {
    position: absolute; right: 6px; top: 7px;
    width: 24px; height: 24px; border-radius: 6px; display: grid; place-items: center; color: var(--faint); opacity: 0;
  }
  .card:hover .del { opacity: 1; }
  .del:hover { color: var(--danger); background: var(--raised); }

  .none { display: block; width: 100%; text-align: left; padding: 6px 10px 6px 28px; font-size: 12.5px; color: var(--faint); border-radius: 8px; }
  .none:hover { color: var(--muted); background: var(--hover); }
  .empty { color: var(--faint); font-size: 13px; padding: 28px 10px; text-align: center; }
  .empty p { margin: 0 0 12px; }
  .cta { padding: 8px 14px; border-radius: 9px; border: 1px solid var(--border); color: var(--text); font-size: 13px; }
  .cta:hover { background: var(--hover); }

  footer { position: relative; border-top: 1px solid var(--border); padding: 6px 8px; }
  .who-btn { flex: 1; min-width: 0; height: 32px; display: flex; align-items: center; gap: 8px; padding: 0 6px; border-radius: 9px; text-align: left; }
  .who-btn:hover, .who-btn.on { background: var(--hover); }
  .updown { flex: none; color: var(--faint); }
  .acct-menu {
    position: absolute; left: 8px; right: 8px; bottom: calc(100% - 4px); z-index: 30; padding: 6px;
    background: var(--panel); border: 1px solid var(--border); border-radius: 12px; box-shadow: 0 18px 50px rgb(0 0 0 / 0.55);
  }
  .pop-sec.first { margin: 4px 8px 6px; }
  .acct { width: 100%; display: flex; align-items: center; gap: 10px; padding: 7px 8px; border-radius: 8px; text-align: left; font-size: 13px; }
  .acct:hover { background: var(--hover); }
  .acct .check { flex: none; color: var(--accent); }
  .acct.action { color: var(--muted); gap: 12px; padding-left: 13px; }
  .acct.action:hover { color: var(--text); }
  .avatar.sm { width: 26px; height: 26px; font-size: 11.5px; }
  .avatar.off { background: var(--raised); color: var(--faint); }
  .sep { height: 1px; background: var(--border); margin: 6px 4px; }
  .who { display: flex; align-items: center; gap: 2px; }
  .who .email { flex: 1; min-width: 0; font-size: 13px; color: var(--muted); }
  .who-btn:hover .email, .who-btn.on .email { color: var(--text); }
  .meter {
    flex: none; height: 30px; padding: 0 7px; border-radius: 8px; display: flex; align-items: center; gap: 5px;
    font-size: 11.5px; font-weight: 550; color: var(--muted); font-variant-numeric: tabular-nums;
  }
  .meter:hover, .meter.on { background: var(--hover); color: var(--text); }
  .meter svg { rotate: -90deg; }
  .meter circle { fill: none; stroke-width: 4; }
  .meter .track { stroke: var(--raised); }
  .meter .fill { stroke: var(--accent); stroke-linecap: round; transition: stroke-dasharray 0.4s; }
  .meter .fill.week { stroke: color-mix(in srgb, var(--accent) 55%, var(--muted)); }
  .meter.hot .fill { stroke: var(--danger); }
  .avatar {
    width: 22px; height: 22px; border-radius: 50%; flex: none; display: grid; place-items: center;
    background: var(--accent); color: var(--accent-ink); font-weight: 650; font-size: 10.5px;
  }
  .who-meta { flex: 1; min-width: 0; }
  .email { font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .icon-btn.on { background: var(--raised); color: var(--text); }
  .plan { font-size: 11px; color: var(--faint); text-transform: capitalize; }
</style>
