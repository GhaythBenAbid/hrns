<script lang="ts">
  import { app, projectOf, type Pane } from "./store.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";

  let { pane, autofocus = false }: { pane: Pane; autofocus?: boolean } = $props();
  let text = $state("");
  let box: HTMLTextAreaElement;
  let menu = $state<"model" | "effort" | "access" | "workspace" | null>(null);

  const accessModes = [
    { id: "read-only", label: "Read only", desc: "Codex can read files. Asks before editing or running commands." },
    { id: "auto", label: "Auto", desc: "Edits files and runs commands in the project. Asks for anything outside it or for network." },
    { id: "full", label: "Full access", desc: "Can edit anything and use the network without asking. Use with care." },
  ] as const;
  const accessLabel = $derived(accessModes.find((m) => m.id === app.access)?.label ?? "Auto");
  // Existing chats keep their folder (their project, for chats in a worktree); new chats use the selected one.
  const folder = $derived(pane.current ? projectOf(pane.current) : app.folder);
  const wt = $derived(pane.current?.worktree);
  // New chats in a git project choose where to work; refresh its branches when the project changes.
  $effect(() => {
    if (!pane.current) app.loadRepo(app.folder);
  });
  const newInWorktree = $derived(!pane.current && !!app.repo && app.workspace === "worktree");
  const shortBranch = (b: string) => b.replace(/^hrns\//, "");
  const folderName = $derived(folder ? folder.split("/").filter(Boolean).pop() : null);

  const streaming = $derived(!!pane.current && !!app.streaming[pane.current.id]);
  const efforts = $derived(app.currentModel?.supportedReasoningEfforts ?? []);
  const effortLabel = $derived(app.effort ?? app.currentModel?.defaultReasoningEffort ?? "default");

  // Menus open upward: cap their height to the room above the composer so none is cut off under the top bar.
  let menuMax = $state(380);
  function toggle(e: MouseEvent, which: NonNullable<typeof menu>) {
    e.stopPropagation();
    if (menu === which) return (menu = null);
    const btn = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const paneTop = Math.max(0, box?.closest(".pane")?.getBoundingClientRect().top ?? 0);
    // 52px: the window's title bar area (traffic lights, header buttons); 8px: gap above the button.
    menuMax = Math.max(160, Math.min(380, btn.top - paneTop - 52 - 8 - 8));
    menu = which;
  }

  export function focus() {
    box?.focus();
  }
  export function fill(s: string) {
    text = s;
    queueMicrotask(() => { resize(); box.focus(); });
  }

  function resize() {
    box.style.height = "auto";
    box.style.height = Math.min(box.scrollHeight, 260) + "px";
  }

  function submit() {
    if (streaming || !text.trim()) return;
    app.send(text, pane);
    text = "";
    queueMicrotask(resize);
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      submit();
    }
  }

  $effect(() => {
    if (autofocus) box?.focus();
  });
</script>

<svelte:window onclick={() => (menu = null)} />

<div class="composer">
  <textarea
    bind:this={box}
    bind:value={text}
    oninput={resize}
    onkeydown={onkey}
    rows="1"
    placeholder="Ask anything…"
    spellcheck="true"
  ></textarea>

  <div class="row">
    <div class="pick model">
      <button class="pill" title={app.currentModel?.displayName} onclick={(e) => toggle(e, "model")}>
        <span class="lbl">{app.currentModel?.displayName ?? "Select model"}</span>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
      </button>
      {#if menu === "model"}
        <div class="menu" role="menu" tabindex="-1" style:max-height="{menuMax}px" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
          {#each app.models as m (m.id)}
            <button class="opt" class:sel={m.id === app.model} onclick={() => { app.setModel(m.id); menu = null; }}>
              <div class="name">{m.displayName}</div>
              <div class="desc">{m.description}</div>
              {#if m.id === app.model}<svg class="check" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><path d="m5 12 5 5 9-10" /></svg>{/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    {#if efforts.length}
      <div class="pick">
        <button class="pill" title="Reasoning: {effortLabel}" onclick={(e) => toggle(e, "effort")}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 3a6 6 0 0 0-4 10.5V17h8v-3.5A6 6 0 0 0 12 3ZM9 21h6" /></svg>
          <span class="cap lbl hide-sm">{effortLabel}</span>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
        </button>
        {#if menu === "effort"}
          <div class="menu small" role="menu" tabindex="-1" style:max-height="{menuMax}px" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
            {#each efforts as e (e.reasoningEffort)}
              <button class="opt" class:sel={e.reasoningEffort === effortLabel} onclick={() => { app.setEffort(e.reasoningEffort); menu = null; }}>
                <div class="name cap">{e.reasoningEffort}</div>
                <div class="desc">{e.description}</div>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <div class="pick">
      <button class="pill" class:full={app.access === "full"} title="Access: {accessLabel}" onclick={(e) => toggle(e, "access")}>
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 4 6v6c0 4.5 3.4 8.2 8 9 4.6-.8 8-4.5 8-9V6Z" /></svg>
        <span class="lbl hide-sm">{accessLabel}</span>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
      </button>
      {#if menu === "access"}
        <div class="menu" role="menu" tabindex="-1" style:max-height="{menuMax}px" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
          {#each accessModes as m (m.id)}
            <button class="opt" class:sel={m.id === app.access} onclick={() => { app.setAccess(m.id); menu = null; }}>
              <div class="name">{m.label}</div>
              <div class="desc">{m.desc}</div>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <div class="spacer"></div>

    {#if wt}
      <span class="pill branch" title="Works in its own worktree on {wt.branch}, branched from {wt.base}">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="6" cy="5" r="2" /><circle cx="6" cy="19" r="2" /><circle cx="18" cy="7" r="2" /><path d="M6 7v10M18 9c0 4-6 3-11.5 8" /></svg>
        <span class="lbl hide-sm">{shortBranch(wt.branch)}</span>
      </span>
    {:else if !pane.current && app.repo}
      <div class="pick">
        <button
          class="pill"
          class:on={newInWorktree}
          title={newInWorktree ? `New worktree from ${app.base}: its own folder and branch, so other chats' edits don't collide` : "Works directly in the project folder"}
          onclick={(e) => toggle(e, "workspace")}
        >
          {#if newInWorktree}
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="6" cy="5" r="2" /><circle cx="6" cy="19" r="2" /><circle cx="18" cy="7" r="2" /><path d="M6 7v10M18 9c0 4-6 3-11.5 8" /></svg>
            <span class="lbl hide-sm">Worktree · {app.base}</span>
          {:else}
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2.5" /><path d="M3 9h18" /></svg>
            <span class="lbl hide-sm">Local</span>
          {/if}
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
        </button>
        {#if menu === "workspace"}
          <div class="menu right" role="menu" tabindex="-1" style:max-height="{menuMax}px" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
            <button class="opt" class:sel={app.workspace === "local"} onclick={() => { app.setWorkspace("local"); menu = null; }}>
              <div class="name">Local</div>
              <div class="desc">Work in the project folder{app.repo.current ? `, on ${app.repo.current}` : ""}.</div>
            </button>
            <div class="sep">New worktree from</div>
            <div class="desc pad">Its own folder and branch, so chats running side by side never edit the same files.</div>
            {#each app.repo.branches as b (b)}
              <button class="opt branch-opt" class:sel={app.workspace === "worktree" && app.base === b} onclick={() => { app.setWorkspace("worktree", b); menu = null; }}>
                <div class="name">{b}{#if b === app.repo.current}<span class="tag">current</span>{/if}</div>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <button
      class="pill folder"
      title={folder ?? "Choose a project folder"}
      disabled={!!pane.current}
      onclick={() => app.pickFolder()}
    >
      {#if folder}<ProjectIcon path={folder} size={15} />{:else}<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" /></svg>{/if}
      <span class="lbl hide-xs">{folderName ?? "Choose folder"}</span>
    </button>

    {#if streaming}
      <button class="send stop" title="Stop (Esc)" onclick={() => app.stop(pane)}>
        <svg width="12" height="12" viewBox="0 0 24 24"><rect x="5" y="5" width="14" height="14" rx="2.5" fill="currentColor" /></svg>
      </button>
    {:else}
      <button class="send" title="Send (Enter)" disabled={!text.trim() || !app.model} onclick={submit}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M12 19V5M5 12l7-7 7 7" /></svg>
      </button>
    {/if}
  </div>
</div>

<style>
  .composer {
    container-type: inline-size;
    background: var(--panel); border: 1px solid var(--border); border-radius: 20px;
    padding: 12px 12px 10px 16px;
    box-shadow: 0 0 0 1px rgb(255 255 255 / 0.02), 0 12px 40px rgb(0 0 0 / 0.6);
  }
  .composer:focus-within { border-color: color-mix(in srgb, var(--accent) 45%, var(--border)); }
  textarea {
    width: 100%; resize: none; border: 0; outline: 0; background: none;
    font-size: 15px; line-height: 1.5; max-height: 260px; padding: 2px 0 6px;
    user-select: text; -webkit-user-select: text;
  }
  textarea::placeholder { color: var(--faint); }
  .row { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .spacer { flex: 1; min-width: 0; }
  .pick { position: relative; flex: none; }
  .pick.model { flex: 0 1 auto; min-width: 0; }
  .pick.model .pill { max-width: 100%; }
  .pill {
    height: 30px; padding: 0 10px; border-radius: 999px; display: flex; align-items: center; gap: 6px;
    font-size: 13px; color: var(--muted); border: 1px solid transparent; white-space: nowrap; min-width: 0;
  }
  .pill svg { flex: none; }
  .lbl { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; min-width: 0; }
  .send { flex: none; }
  .pill:hover:not(:disabled) { background: var(--hover); color: var(--text); }
  .pill.full { color: var(--accent); }
  .folder { flex: 0 1 auto; max-width: 200px; }
  .pill:disabled { cursor: default; }
  .folder:disabled { opacity: 0.8; }
  .cap { text-transform: capitalize; }
  .menu {
    position: absolute; bottom: calc(100% + 8px); left: 0; z-index: 20;
    width: 320px; max-width: calc(100cqw - 8px); max-height: 380px; overflow-y: auto; padding: 6px;
    background: var(--panel); border: 1px solid var(--border); border-radius: 14px;
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.35);
  }
  .menu.small { width: 280px; }
  .menu.right { left: auto; right: 0; }
  .sep { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--faint); padding: 10px 10px 2px; border-top: 1px solid var(--border); margin-top: 4px; }
  .pad { padding: 0 10px 6px; }
  .branch-opt { padding-top: 6px; padding-bottom: 6px; }
  .branch-opt .name { font-family: var(--mono, ui-monospace, monospace); font-size: 12.5px; font-weight: 500; display: flex; align-items: center; gap: 8px; }
  .tag { font-family: inherit; font-size: 10.5px; color: var(--muted); border: 1px solid var(--border); border-radius: 999px; padding: 0 6px; }
  .pill.on { color: var(--accent); }
  .branch { flex: 0 1 auto; max-width: 180px; cursor: default; }
  .opt { position: relative; width: 100%; text-align: left; padding: 8px 30px 8px 10px; border-radius: 9px; display: block; }
  .opt:hover { background: var(--hover); }
  .opt.sel .name { color: var(--accent); }
  .name { font-size: 13.5px; font-weight: 550; }
  .desc { font-size: 12px; color: var(--muted); margin-top: 1px; line-height: 1.4; }
  .check { position: absolute; right: 10px; top: 11px; color: var(--accent); }
  .send {
    width: 34px; height: 34px; border-radius: 50%; display: grid; place-items: center;
    background: var(--text); color: var(--bg);
  }
  .send:disabled { opacity: 0.25; }
  .send.stop { background: var(--text); }

  /* Narrow composers (split view, small windows): drop labels to icons, keep everything on one row. */
  @container (max-width: 520px) {
    .hide-sm { display: none; }
    .pill { padding: 0 8px; }
    .folder { max-width: 140px; }
  }
  @container (max-width: 380px) {
    .hide-xs { display: none; }
    .row { gap: 2px; }
    .menu { left: -8px; }
  }
</style>
