<script lang="ts">
  import { tick, untrack } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { invoke } from "@tauri-apps/api/core";
  import { app, projectOf, type Pane } from "./store.svelte";
  import Composer from "./Composer.svelte";
  import MessageView from "./MessageView.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import AnimatedTitle from "./AnimatedTitle.svelte";
  import DiffButton from "./DiffButton.svelte";

  let { pane }: { pane: Pane } = $props();

  const focused = $derived(app.pane === pane);
  // Read once: refocusing the composer on every pane switch would steal clicks meant for selecting text.
  const autofocus = untrack(() => focused);
  const split = $derived(app.panes.length > 1);

  let scroller = $state<HTMLElement>();
  let composer = $state<Composer>();
  let stick = true;

  export function focus() {
    composer?.focus();
  }

  const suggestions = [
    { title: "Explain this codebase", desc: "Map the structure and key modules", icon: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM16.2 7.8l-2.1 6.3-6.3 2.1 2.1-6.3Z", prompt: "Explain how this codebase is structured: the main modules, how they fit together, and where to start reading." },
    { title: "Find and fix a bug", desc: "Review the current changes", icon: "M8 2l1.9 1.9M16 2l-1.9 1.9M9 7.1V6a3 3 0 1 1 6 0v1.1M12 20c-3.3 0-6-2.7-6-6v-3a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v3c0 3.3-2.7 6-6 6ZM12 20v-9M6.5 13H3M21 13h-3.5", prompt: "Look at the current uncommitted changes, find any bugs, and fix them." },
    { title: "Write tests", desc: "Cover the most important module", icon: "M9 11l3 3L22 4M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11", prompt: "Find the most important module that lacks tests and write focused tests for it." },
    { title: "Summarize recent work", desc: "Read the git history", icon: "M12 8v4l3 3M3.05 11a9 9 0 1 1 .5 4M3 4v5h5", prompt: "Summarize the recent git history: what changed, why, and anything that looks unfinished." },
  ];

  const folderName = $derived(app.folder?.split("/").filter(Boolean).pop() ?? "");
  const folderParent = $derived(app.folder ? app.folder.replace(/\/[^/]+\/?$/, "").replace(/^\/Users\/[^/]+/, "~") : "");
  const recent = $derived(app.folder ? app.threads.filter((t) => projectOf(t) === app.folder).slice(0, 3) : []);
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

  function onscroll() {
    if (!scroller) return;
    stick = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;
  }

  // Follow the stream only while the user is at the bottom.
  $effect(() => {
    pane.live?.parts.length;
    pane.live?.parts.at(-1);
    pane.live?.approvals.length;
    pane.current?.messages.length;
    if (stick) tick().then(() => scroller && (scroller.scrollTop = scroller.scrollHeight));
  });

  $effect(() => {
    pane.current?.id;
    stick = true;
    if (untrack(() => focused)) composer?.focus();
  });

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

<!-- svelte-ignore a11y_no_static_element_interactions -->
<section class="pane" class:split class:focused onpointerdowncapture={() => app.focus(pane)} onfocusincapture={() => app.focus(pane)}>
  {#if split}
    <div class="pane-head">
      <span class="pane-title"><AnimatedTitle text={pane.current?.title ?? "New chat"} id={pane.current?.id} /></span>
      <DiffButton {pane} />
      <button class="icon-btn close" title="Close pane" onclick={() => app.closePane(pane)}>
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 6 12 12M18 6 6 18" /></svg>
      </button>
    </div>
  {/if}

  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scroll" bind:this={scroller} {onscroll} {onclick}>
    {#if pane.current}
      <div class="thread">
        {#each pane.current.messages as m (m.id)}
          <MessageView msg={m} />
        {/each}
        {#if pane.live}
          <MessageView msg={{ role: "assistant", text: "", parts: pane.live.parts }} live approvals={pane.live.approvals} notice={pane.live.notice} />
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
              <ProjectIcon path={app.folder} size={16} />
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
            <Composer bind:this={composer} {pane} {autofocus} />
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
                <button class="recent-row" onclick={() => app.open(t.id, pane)}>
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

  {#if pane.current}
    <div class="dock">
      <Composer bind:this={composer} {pane} {autofocus} />
      <div class="hint">AI can make mistakes. Check important info.</div>
    </div>
  {/if}
</section>

<style>
  .pane { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; position: relative; }
  .pane-head {
    height: 34px; flex: none; display: flex; align-items: center; gap: 8px; padding: 0 8px 0 16px;
    border-bottom: 1px solid var(--border);
  }
  .pane-title {
    flex: 1; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;
    font-size: 12.5px; font-weight: 550; color: var(--faint);
  }
  .pane.focused .pane-title { color: var(--text); }
  .pane.split.focused .pane-head { box-shadow: inset 0 -1px 0 color-mix(in srgb, var(--accent) 55%, transparent); }
  .close { width: 24px; height: 24px; opacity: 0.6; }
  .close:hover { opacity: 1; }

  .scroll { flex: 1; overflow-y: auto; overflow-x: hidden; }
  .thread { max-width: var(--thread-max, 780px); min-width: 0; margin: 0 auto; padding: 8px 28px 40px; }
  .split .thread { padding-left: 20px; padding-right: 20px; }

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
  .split .cards { grid-template-columns: 1fr; }
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

  .dock { flex: none; width: 100%; max-width: var(--dock-max, 820px); margin: 0 auto; padding: 0 20px 10px; }
  .split .dock { padding: 0 14px 10px; }
  .hint { text-align: center; font-size: 11.5px; color: var(--faint); padding-top: 8px; }
</style>
