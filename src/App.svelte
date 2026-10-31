<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { app, accents, fonts, monoFonts, type AccentPreset } from "./lib/store.svelte";
  import { drag, setDropTarget } from "./lib/drag.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import ChatPane from "./lib/ChatPane.svelte";
  import Splash from "./lib/Splash.svelte";
  import ProjectFinder from "./lib/ProjectFinder.svelte";
  import AnimatedTitle from "./lib/AnimatedTitle.svelte";
  import DiffButton from "./lib/DiffButton.svelte";
  import DiffSidebar from "./lib/DiffSidebar.svelte";
  import TerminalPanel from "./lib/TerminalPanel.svelte";
  import { terms } from "./lib/terminal.svelte";
  import { refreshDiff, watchDiffs } from "./lib/gitdiff.svelte";
  import Settings from "./lib/Settings.svelte";
  import Usage from "./lib/Usage.svelte";

  const scales = { small: 0.92, default: 1, large: 1.1 };
  $effect(() => {
    document.documentElement.style.setProperty("--scale", String(scales[app.textSize]));
  });
  $effect(() => {
    const root = document.documentElement.style;
    root.setProperty("--font", fonts[app.font].stack);
    root.setProperty("--mono", monoFonts[app.monoFont].stack);
  });

  // The native window follows the theme too (title bar, menus, dialogs), and with "system"
  // it tracks macOS, which is what the media query below then reports.
  let scheme = $state<"light" | "dark">("dark");
  $effect(() => {
    const theme = app.theme;
    getCurrentWindow().setTheme(theme === "system" ? null : theme).catch(() => {});
    const dark = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => (scheme = theme === "system" ? (dark.matches ? "dark" : "light") : theme);
    apply();
    dark.addEventListener("change", apply);
    return () => dark.removeEventListener("change", apply);
  });
  $effect(() => {
    document.documentElement.dataset.theme = scheme;
  });

  // Text on accent-filled surfaces (avatars, badges) flips to stay readable on light or dark accents.
  function inkFor(hex: string) {
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.55 ? "#0b0814" : "#ffffff";
  }
  $effect(() => {
    const a = app.accent;
    const color = a.startsWith("#") ? a : (accents[a as AccentPreset] ?? accents.violet)[scheme];
    const root = document.documentElement.style;
    root.setProperty("--accent", color);
    root.setProperty("--accent-ink", inkFor(color));
  });

  let mainEl = $state<HTMLElement>();

  // Sidebar toggle animates with transforms only (GPU), never width: animating layout
  // would re-flow the whole chat every frame. Main snaps to its new size once, then
  // slides from where its centre used to be (FLIP).
  const SIDEBAR = 288;
  const EASE = "cubic-bezier(0.32, 0.72, 0, 1)";
  // Settings takes over the whole window, like Codex.
  const sidebarShown = $derived(app.sidebarOpen && app.view !== "settings");
  let wasShown: boolean | null = null;
  $effect(() => {
    const shown = sidebarShown;
    if (wasShown !== null && shown !== wasShown && !matchMedia("(prefers-reduced-motion: reduce)").matches) {
      mainEl?.animate(
        [{ transform: `translateX(${shown ? -SIDEBAR / 2 : SIDEBAR / 2}px)` }, { transform: "translateX(0)" }],
        { duration: 280, easing: EASE },
      );
    }
    wasShown = shown;
  });

  function toggleSidebar() {
    if (app.view === "settings") {
      app.view = "chat";
      if (!app.sidebarOpen) app.toggleSidebar();
    } else {
      app.toggleSidebar();
    }
  }

  let panes = $state<ChatPane[]>([]);
  const focusComposer = () => tick().then(() => panes[app.focused]?.focus());
  $effect(() => setDropTarget(mainEl ?? null));

  onMount(() => {
    app.init();
    terms.init();
  });

  const paneDir = () => (app.pane.current ? app.pane.current.cwd : app.folder);
  function toggleTerminal() {
    if (terms.open) terms.setOpen(false);
    else terms.show(paneDir());
  }

  // Uncommitted changes of every project on screen, for the header's +/− button.
  const shownDirs = () => app.panes.map((p) => (p.current ? p.current.cwd : app.folder));
  $effect(() => {
    shownDirs().forEach(refreshDiff);
  });
  $effect(() => watchDiffs(shownDirs));
  // An agent turn finishing usually means files changed.
  $effect(() => {
    Object.keys(app.streaming).length;
    untrack(() => shownDirs().forEach(refreshDiff));
  });

  function onkeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === "n") { e.preventDefault(); app.newChat(); focusComposer(); }
    else if (mod && e.key === "o" && !app.finder) { e.preventDefault(); app.pickFolder().then((dir) => { if (dir) { app.newChatIn(dir); focusComposer(); } }); }
    else if (mod && e.key === "b") { e.preventDefault(); toggleSidebar(); }
    else if (mod && (e.key === "j" || e.key === "`")) { e.preventDefault(); toggleTerminal(); }
    else if (mod && e.key === ",") { e.preventDefault(); app.view = app.view === "settings" ? "chat" : "settings"; }
    else if (mod && e.key === "w" && app.panes.length > 1) { e.preventDefault(); app.closePane(app.pane); }
    else if (e.key === "Escape" && app.view !== "chat") app.view = "chat";
    else if (e.key === "Escape" && app.diffOpen) app.setDiffOpen(false);
    else if (e.key === "Escape" && app.current && app.streaming[app.current.id]) app.stop();
    else if (e.key === "Enter" && mod && app.live?.approvals.length) { e.preventDefault(); app.respond(app.live.approvals[0].key, "accept"); }
  }

  // Drag the divider between panes to resize them.
  function resize(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const r = el.parentElement!.getBoundingClientRect();
    const move = (ev: PointerEvent) => app.setSplitRatio((ev.clientX - r.left) / r.width);
    el.addEventListener("pointermove", move);
    el.addEventListener("lostpointercapture", () => el.removeEventListener("pointermove", move), { once: true });
  }
</script>

<svelte:window {onkeydown} />

<div class="shell">
  <!-- Window controls: fixed next to the traffic lights, same spot with the sidebar open or closed. -->
  <div class="chrome">
    <button class="icon-btn" title="{app.sidebarOpen ? 'Hide' : 'Show'} sidebar (⌘B)" onclick={toggleSidebar}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="16" rx="3" /><path d="M9 4v16" /></svg>
    </button>
    <button class="icon-btn" title="New chat (⌘N)" onclick={() => { app.newChat(); focusComposer(); }}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" /></svg>
    </button>
  </div>

  <div class="side" class:closed={!sidebarShown} inert={!sidebarShown}>
    <Sidebar />
  </div>

  <main bind:this={mainEl} class:full={!sidebarShown}>
    <header data-tauri-drag-region class:inset={!sidebarShown}>
      <div class="title" data-tauri-drag-region>{#if app.view === "settings"}Settings{:else if app.view === "chat" && app.panes.length === 1}<AnimatedTitle text={app.current?.title ?? ""} id={app.current?.id} />{/if}</div>
      {#if app.view === "chat" && app.panes.length === 1 && app.ready && app.account}
        <div class="header-actions">
          <button class="icon-btn term-btn" class:on={terms.open} title="{terms.open ? 'Hide' : 'Show'} terminal (⌘J)" aria-label="Terminal" aria-pressed={terms.open} onclick={toggleTerminal}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="3" /><path d="m7 10 3 2-3 2M13 15h4" /></svg>
            {#if terms.list.length}<span class="badge">{terms.list.length}</span>{/if}
          </button>
          <DiffButton pane={app.panes[0]} />
        </div>
      {/if}
    </header>

    {#if app.view === "settings"}
      <Settings />
    {:else if app.view === "usage"}
      <Usage />
    {:else if !app.ready}
      <Splash />
    {:else if !app.account}
      <div class="center">
        <div class="login">
          <h1>Sign in to start chatting</h1>
          <p>hrns uses your ChatGPT subscription through the Codex CLI.</p>
          <button class="primary" onclick={() => app.login()}>Sign in with ChatGPT</button>
          {#if app.error}<div class="err">{app.error}</div>{/if}
        </div>
      </div>
    {:else}
      <div class="stack">
      <div class="work">
        <div class="panes">
          {#each app.panes as pane, i (pane)}
            {#if i > 0}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="divider" title="Drag to resize" onpointerdown={resize} ondblclick={() => app.setSplitRatio(0.5)}></div>
            {/if}
            <div class="slot" style:flex="{app.panes.length > 1 ? (i === 0 ? app.splitRatio : 1 - app.splitRatio) : 1} 1 0">
              <ChatPane bind:this={panes[i]} {pane} />
            </div>
          {/each}
        </div>
        {#if app.diffOpen}
          <DiffSidebar dir={app.pane.current ? app.pane.current.cwd : app.folder} />
        {/if}
      </div>
      {#if terms.open}
        <TerminalPanel cwd={paneDir()} />
      {/if}
      </div>
    {/if}

    {#if drag.id && app.ready && app.account}
      <div class="drop" class:pair={app.panes.length > 1}>
        {#each ["left", "right"] as side (side)}
          <div class="zone" class:on={drag.zone === side}>
            <span>{app.panes.length > 1 ? `Open in ${side} pane` : `Split ${side}`}</span>
          </div>
        {/each}
      </div>
    {/if}
  </main>
</div>

{#if app.finder}
  <ProjectFinder />
{/if}

{#if drag.id}
  <div class="ghost" style:left="{drag.x + 12}px" style:top="{drag.y + 10}px">{drag.title}</div>
{/if}

<style>
  .shell { display: flex; height: 100%; position: relative; }
  .chrome {
    position: absolute; z-index: 50; top: 11px; left: 80px;
    display: flex; gap: 2px;
  }
  .chrome .icon-btn { width: 30px; height: 30px; }
  .side {
    position: absolute; z-index: 20; top: 0; bottom: 0; left: 0; width: 288px;
    transition: transform 0.28s cubic-bezier(0.32, 0.72, 0, 1);
    will-change: transform;
  }
  .side.closed { transform: translateX(-100%); }
  @media (prefers-reduced-motion: reduce) {
    .side { transition: none !important; }
  }
  .shell { overflow: hidden; }
  main { flex: 1; min-width: 0; display: flex; flex-direction: column; position: relative; margin-left: 288px; }
  main.full { margin-left: 0; }
  header {
    height: 52px; flex: none; display: flex; align-items: center; gap: 2px; padding: 0 16px;
  }
  header.inset { padding-left: 156px; }
  .header-actions { position: absolute; right: 14px; top: 13px; display: flex; gap: 6px; }
  .title { flex: 1; text-align: center; font-size: 13px; font-weight: 550; color: var(--muted); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; padding: 0 60px; }
  header.inset .title { padding-left: 0; margin-left: -140px; }

  .stack { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .work { flex: 1; min-height: 0; display: flex; }
  .term-btn { position: relative; width: 30px; height: 30px; }
  .term-btn.on { color: var(--accent); background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .badge {
    position: absolute; top: 2px; right: 1px; min-width: 13px; height: 13px; padding: 0 3px; border-radius: 7px;
    font-size: 9px; font-weight: 700; line-height: 13px; text-align: center; color: var(--accent-ink); background: var(--accent);
  }
  .panes { flex: 1; min-width: 0; min-height: 0; display: flex; }
  .slot { min-width: 0; display: flex; }
  .divider {
    flex: none; width: 9px; margin: 0 -4px; z-index: 5; position: relative; cursor: col-resize;
  }
  .divider::after {
    content: ""; position: absolute; top: 0; bottom: 0; left: 4px; width: 1px; background: var(--border);
    transition: background 0.12s;
  }
  .divider:hover::after, .divider:active::after { background: color-mix(in srgb, var(--accent) 60%, var(--border)); }

  .drop {
    position: absolute; inset: 52px 0 0; z-index: 40; display: grid; grid-template-columns: 1fr 1fr; gap: 10px; padding: 10px;
    pointer-events: none; animation: fade 0.12s ease-out;
  }
  .zone {
    display: grid; place-items: center; border-radius: 16px;
    border: 1.5px dashed color-mix(in srgb, var(--accent) 30%, transparent);
    background: color-mix(in srgb, var(--accent) 3%, transparent);
    transition: background 0.12s, border-color 0.12s;
  }
  .zone span {
    padding: 6px 12px; border-radius: 999px; font-size: 12.5px; font-weight: 550; color: var(--muted);
    background: var(--panel); border: 1px solid var(--border);
  }
  .zone.on { border-color: var(--accent); border-style: solid; background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .zone.on span { color: var(--text); }
  @keyframes fade { from { opacity: 0; } }

  .ghost {
    position: fixed; z-index: 200; pointer-events: none; max-width: 240px;
    padding: 7px 12px; border-radius: 10px; font-size: 13px; font-weight: 550;
    overflow: hidden; white-space: nowrap; text-overflow: ellipsis;
    background: var(--raised); border: 1px solid var(--border); box-shadow: 0 12px 34px rgb(0 0 0 / 0.45);
  }
  :global(body.dragging), :global(body.dragging *) { cursor: grabbing !important; }

  .center { flex: 1; display: grid; place-items: center; padding: 24px; }
  .login { text-align: center; max-width: 380px; }
  .login h1 { font-size: 24px; font-weight: 600; margin: 0 0 8px; }
  .login p { color: var(--muted); margin: 0 0 22px; }
  .primary { height: 40px; padding: 0 20px; border-radius: 10px; background: var(--text); color: var(--bg); font-weight: 600; }
  .err { margin-top: 16px; color: var(--danger); font-size: 13px; user-select: text; -webkit-user-select: text; }
</style>
