<script lang="ts">
  import { onMount, tick } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { invoke } from "@tauri-apps/api/core";
  import { app } from "./lib/store.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import Composer from "./lib/Composer.svelte";
  import MessageView from "./lib/MessageView.svelte";
  import Settings from "./lib/Settings.svelte";
  import Usage from "./lib/Usage.svelte";

  const scales = { small: 0.92, default: 1, large: 1.1 };
  $effect(() => {
    document.documentElement.style.setProperty("--scale", String(scales[app.textSize]));
  });

  let scroller = $state<HTMLElement>();
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

  let composer = $state<Composer>();
  let stick = true;

  const suggestions = [
    { title: "Explain this codebase", desc: "Map the structure and key modules", icon: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM16.2 7.8l-2.1 6.3-6.3 2.1 2.1-6.3Z", prompt: "Explain how this codebase is structured: the main modules, how they fit together, and where to start reading." },
    { title: "Find and fix a bug", desc: "Review the current changes", icon: "M8 2l1.9 1.9M16 2l-1.9 1.9M9 7.1V6a3 3 0 1 1 6 0v1.1M12 20c-3.3 0-6-2.7-6-6v-3a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v3c0 3.3-2.7 6-6 6ZM12 20v-9M6.5 13H3M21 13h-3.5", prompt: "Look at the current uncommitted changes, find any bugs, and fix them." },
    { title: "Write tests", desc: "Cover the most important module", icon: "M9 11l3 3L22 4M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11", prompt: "Find the most important module that lacks tests and write focused tests for it." },
    { title: "Summarize recent work", desc: "Read the git history", icon: "M12 8v4l3 3M3.05 11a9 9 0 1 1 .5 4M3 4v5h5", prompt: "Summarize the recent git history: what changed, why, and anything that looks unfinished." },
  ];

  const folderName = $derived(app.folder?.split("/").filter(Boolean).pop() ?? "");
  const folderParent = $derived(app.folder ? app.folder.replace(/\/[^/]+\/?$/, "").replace(/^\/Users\/[^/]+/, "~") : "");
  const recent = $derived(app.folder ? app.threads.filter((t) => t.cwd === app.folder).slice(0, 3) : []);
  let branch = $state<string | null>(null);
  $effect(() => {
    const dir = app.folder;
    branch = null;
    if (dir) invoke<string | null>("git_branch", { path: dir }).then((b) => { if (app.folder === dir) branch = b; });
  });

  function ago(ts: number) {
    const m = Math.round((Date.now() - ts) / 60_000);
    if (m < 1) return "just now";
    if (m < 60) return `${m}m ago`;
    if (m < 1440) return `${Math.round(m / 60)}h ago`;
    return `${Math.round(m / 1440)}d ago`;
  }

  onMount(() => {
    app.init();
  });

  function onscroll() {
    if (!scroller) return;
    stick = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;
  }

  // Follow the stream only while the user is at the bottom.
  $effect(() => {
    app.live?.parts.length;
    app.live?.parts.at(-1);
    app.live?.approvals.length;
    app.current?.messages.length;
    if (stick) tick().then(() => scroller && (scroller.scrollTop = scroller.scrollHeight));
  });

  $effect(() => {
    app.current?.id;
    stick = true;
    composer?.focus();
  });

  function onkeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === "n") { e.preventDefault(); app.newChat(); composer?.focus(); }
    else if (mod && e.key === "b") { e.preventDefault(); toggleSidebar(); }
    else if (mod && e.key === ",") { e.preventDefault(); app.view = app.view === "settings" ? "chat" : "settings"; }
    else if (e.key === "Escape" && app.view !== "chat") app.view = "chat";
    else if (e.key === "Escape" && app.current && app.streaming[app.current.id]) app.stop();
    else if (e.key === "Enter" && mod && app.live?.approvals.length) { e.preventDefault(); app.respond(app.live.approvals[0].key, "accept"); }
  }

  // Copy buttons in code blocks + external links, via delegation.
  function onclick(e: MouseEvent) {
    const el = e.target as HTMLElement;
    const copyBtn = el.closest<HTMLButtonElement>("[data-copy]");
    if (copyBtn) {
      const code = copyBtn.closest(".code")?.querySelector("pre")?.innerText ?? "";
      navigator.clipboard.writeText(code);
      copyBtn.textContent = "Copied";
      setTimeout(() => (copyBtn.textContent = "Copy"), 1200);
      return;
    }
    const a = el.closest<HTMLAnchorElement>("a[href]");
    if (a && /^https?:/.test(a.href)) {
      e.preventDefault();
      openUrl(a.href);
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="shell">
  <!-- Window controls: fixed next to the traffic lights, same spot with the sidebar open or closed. -->
  <div class="chrome">
    <button class="icon-btn" title="{app.sidebarOpen ? 'Hide' : 'Show'} sidebar (⌘B)" onclick={toggleSidebar}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="16" rx="3" /><path d="M9 4v16" /></svg>
    </button>
    <button class="icon-btn" title="New chat (⌘N)" onclick={() => { app.newChat(); composer?.focus(); }}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" /></svg>
    </button>
  </div>

  <div class="side" class:closed={!sidebarShown} inert={!sidebarShown}>
    <Sidebar />
  </div>

  <main bind:this={mainEl} class:full={!sidebarShown}>
    <header data-tauri-drag-region class:inset={!sidebarShown}>
      <div class="title" data-tauri-drag-region>{app.view === "settings" ? "Settings" : app.view === "usage" ? "" : (app.current?.title ?? "")}</div>
    </header>

    {#if app.view === "settings"}
      <Settings />
    {:else if app.view === "usage"}
      <Usage />
    {:else if !app.ready}
      <div class="center"><div class="muted">Starting Codex…</div></div>
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
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="scroll" bind:this={scroller} {onscroll} {onclick}>
        {#if app.current}
          <div class="thread">
            {#each app.current.messages as m (m.id)}
              <MessageView msg={m} />
            {/each}
            {#if app.live}
              <MessageView msg={{ role: "assistant", text: "", parts: app.live.parts }} live approvals={app.live.approvals} notice={app.live.notice} />
            {/if}
          </div>
        {:else}
          <div class="hero">
            <div class="mark" aria-hidden="true">
              <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke-linecap="round" stroke-linejoin="round"><path d="M6 7l5 5-5 5" stroke="currentColor" stroke-width="2.4" /><path d="M13 17h6" stroke="var(--accent)" stroke-width="2.4" /></svg>
            </div>
            <h1>{app.folder ? "What should we build?" : "Pick a project to get started"}</h1>

            {#if app.folder}
              <div class="ctx">
                <button class="ctx-pill" title="{app.folder} — change project" onclick={() => app.pickFolder()}>
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" /></svg>
                  <span class="ctx-name">{folderName}</span>
                  <span class="ctx-path">{folderParent}</span>
                  <svg class="ctx-chev" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
                </button>
                {#if branch}
                  <span class="ctx-pill static" title="Current git branch">
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="6" cy="6" r="2.5" /><circle cx="6" cy="18" r="2.5" /><circle cx="18" cy="8" r="2.5" /><path d="M6 8.5v7M18 10.5c0 4-6 3-11 5.5" /></svg>
                    {branch}
                  </span>
                {/if}
              </div>
            {:else}
              <p class="lead">Codex works inside a folder on your Mac. Choose one to begin — you can switch any time.</p>
              <button class="cta" onclick={() => app.pickFolder()}>
                <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" /></svg>
                Choose project folder
              </button>
            {/if}

            {#if app.folder}
              <div class="hero-composer">
                <Composer bind:this={composer} autofocus />
              </div>

              <div class="cards">
                {#each suggestions as s (s.title)}
                  <button class="card" onclick={() => composer?.fill(s.prompt)}>
                    <span class="card-ico"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d={s.icon} /></svg></span>
                    <span class="card-text">
                      <span class="card-title">{s.title}</span>
                      <span class="card-desc">{s.desc}</span>
                    </span>
                  </button>
                {/each}
              </div>

              {#if recent.length}
                <div class="recent">
                  <div class="recent-head">Recent in {folderName}</div>
                  {#each recent as t (t.id)}
                    <button class="recent-row" onclick={() => app.open(t.id)}>
                      <span class="recent-title">{t.title}</span>
                      <span class="recent-ago">{ago(t.updatedAt)}</span>
                    </button>
                  {/each}
                </div>
              {/if}
            {/if}
          </div>
        {/if}
      </div>

      {#if app.current}
        <div class="dock">
          <Composer bind:this={composer} autofocus />
          <div class="hint">AI can make mistakes. Check important info.</div>
        </div>
      {/if}
    {/if}
  </main>
</div>

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
  .title { flex: 1; text-align: center; font-size: 13px; font-weight: 550; color: var(--muted); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; padding: 0 60px; }
  header.inset .title { padding-left: 0; margin-left: -140px; }

  .scroll { flex: 1; overflow-y: auto; overflow-x: hidden; }
  .thread { max-width: 780px; min-width: 0; margin: 0 auto; padding: 8px 28px 40px; }

  .hero {
    min-height: 100%; max-width: 760px; margin: 0 auto; padding: 32px 28px 48px;
    display: flex; flex-direction: column; align-items: center; justify-content: center;
  }
  .mark {
    width: 52px; height: 52px; border-radius: 16px; display: grid; place-items: center; margin-bottom: 18px;
    background: linear-gradient(160deg, var(--raised), var(--panel)); border: 1px solid var(--border);
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.35);
  }
  .hero h1 { font-size: 28px; font-weight: 650; letter-spacing: -0.02em; margin: 0; text-align: center; }
  .lead { color: var(--muted); font-size: 14px; text-align: center; max-width: 420px; margin: 10px 0 22px; line-height: 1.5; }
  .cta {
    display: flex; align-items: center; gap: 8px; height: 40px; padding: 0 18px; border-radius: 11px;
    background: var(--text); color: var(--bg); font-weight: 600; font-size: 13.5px;
  }
  .cta:hover { opacity: 0.9; }

  .ctx { display: flex; flex-wrap: wrap; justify-content: center; gap: 8px; margin: 14px 0 26px; }
  .ctx-pill {
    display: inline-flex; align-items: center; gap: 7px; height: 30px; padding: 0 12px; border-radius: 999px;
    border: 1px solid var(--border); background: var(--panel); font-size: 13px; color: var(--text); max-width: 420px;
  }
  button.ctx-pill:hover { background: var(--hover); }
  .ctx-pill.static { color: var(--muted); font-family: var(--mono); font-size: 12px; }
  .ctx-name { font-weight: 550; }
  .ctx-path { color: var(--faint); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; min-width: 0; }
  .ctx-chev { color: var(--faint); flex: none; }

  .hero-composer { width: 100%; }

  .cards { width: 100%; display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-top: 16px; }
  @media (max-width: 720px) { .cards { grid-template-columns: 1fr; } }
  .card {
    display: flex; align-items: flex-start; gap: 12px; padding: 13px 14px; border-radius: 14px; text-align: left;
    border: 1px solid var(--border); background: color-mix(in srgb, var(--panel) 60%, transparent);
  }
  .card:hover { background: var(--hover); border-color: color-mix(in srgb, var(--accent) 30%, var(--border)); }
  .card:hover .card-ico { color: var(--accent); }
  .card-ico {
    flex: none; width: 30px; height: 30px; border-radius: 9px; display: grid; place-items: center;
    background: var(--raised); color: var(--muted); transition: color 0.12s;
  }
  .card-text { display: grid; gap: 2px; min-width: 0; }
  .card-title { font-size: 13.5px; font-weight: 550; }
  .card-desc { font-size: 12.5px; color: var(--muted); }

  .recent { width: 100%; margin-top: 26px; }
  .recent-head { font-size: 11px; font-weight: 650; letter-spacing: 0.06em; text-transform: uppercase; color: var(--faint); padding: 0 10px 6px; }
  .recent-row { width: 100%; display: flex; align-items: center; gap: 12px; padding: 8px 10px; border-radius: 9px; font-size: 13.5px; text-align: left; }
  .recent-row:hover { background: var(--hover); }
  .recent-title { flex: 1; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; color: color-mix(in srgb, var(--text) 85%, transparent); }
  .recent-ago { flex: none; font-size: 12px; color: var(--faint); }

  .dock { flex: none; width: 100%; max-width: 820px; margin: 0 auto; padding: 0 20px 10px; }
  .hint { text-align: center; font-size: 11.5px; color: var(--faint); padding-top: 8px; }

  .center { flex: 1; display: grid; place-items: center; padding: 24px; }
  .muted { color: var(--muted); }
  .login { text-align: center; max-width: 380px; }
  .login h1 { font-size: 24px; font-weight: 600; margin: 0 0 8px; }
  .login p { color: var(--muted); margin: 0 0 22px; }
  .primary { height: 40px; padding: 0 20px; border-radius: 10px; background: var(--text); color: var(--bg); font-weight: 600; }
  .err { margin-top: 16px; color: var(--danger); font-size: 13px; user-select: text; -webkit-user-select: text; }
</style>
