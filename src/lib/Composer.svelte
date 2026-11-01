<script lang="ts">
  import { app, projectOf, type Pane } from "./store.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import AttachmentChip from "./AttachmentChip.svelte";
  import { savePasted, toAttachment, type Attachment } from "./attachments";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";

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
  // Compare mode needs a git project: each model works in its own worktree.
  const canCompare = $derived(!pane.current && !!app.repo && !!app.base);
  const comparing = $derived(canCompare ? app.compare.filter((id) => id !== app.model) : []);
  const newInWorktree = $derived(!pane.current && !!app.repo && (app.workspace === "worktree" || comparing.length > 0));
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

  // Attachments: the paperclip, pasted images/files, or files dropped on the window.
  let files = $state<Attachment[]>([]);
  let dropping = $state(false);
  let attachError = $state<string | null>(null);
  function addPaths(paths: string[]) {
    const have = new Set(files.map((f) => f.path));
    files = [...files, ...paths.filter((p) => !have.has(p)).map(toAttachment)];
  }
  async function pickFiles() {
    const r = await openDialog({ multiple: true, title: "Attach files", defaultPath: folder ?? undefined });
    if (r) addPaths(Array.isArray(r) ? r : [r]);
    box?.focus();
  }
  async function onpaste(e: ClipboardEvent) {
    const pasted = [...(e.clipboardData?.files ?? [])];
    if (!pasted.length) return;
    e.preventDefault();
    attachError = null;
    for (const f of pasted) {
      try {
        const a = await savePasted(f);
        files = [...files, a];
      } catch (err) {
        attachError = String(err);
      }
    }
  }
  // Drops go to the focused pane's composer.
  $effect(() => {
    let unlisten: (() => void) | undefined;
    let gone = false;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (app.pane !== pane || app.view !== "chat") return (dropping = false);
        const p = e.payload;
        if (p.type === "enter" || p.type === "over") dropping = true;
        else if (p.type === "leave") dropping = false;
        else if (p.type === "drop") {
          dropping = false;
          addPaths(p.paths);
          box?.focus();
        }
      })
      .then((u) => (gone ? u() : (unlisten = u)));
    return () => {
      gone = true;
      unlisten?.();
    };
  });

  function submit() {
    if (streaming || (!text.trim() && !files.length)) return;
    app.send(text, pane, files.map((f) => f.path));
    text = "";
    files = [];
    attachError = null;
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

<div class="composer" class:dropping>
  {#if files.length || attachError}
    <div class="files">
      {#each files as f (f.path)}
        <AttachmentChip a={f} onremove={() => (files = files.filter((x) => x !== f))} />
      {/each}
      {#if attachError}<span class="ferr">{attachError}</span>{/if}
    </div>
  {/if}
  {#if dropping}<div class="drophint">Drop to attach</div>{/if}
  <textarea
    bind:this={box}
    bind:value={text}
    oninput={resize}
    onkeydown={onkey}
    {onpaste}
    rows="1"
    placeholder="Ask anything…"
    spellcheck="true"
  ></textarea>

  <div class="row">
    <button class="pill clip" title="Attach files or images (or paste / drop them)" onclick={pickFiles}>
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m21 11-8.6 8.6a5.5 5.5 0 0 1-7.8-7.8l8.6-8.6a3.7 3.7 0 0 1 5.2 5.2l-8.6 8.6a1.8 1.8 0 0 1-2.6-2.6L15 6.6" /></svg>
    </button>
    <div class="pick model">
      <button class="pill" title={app.currentModel?.displayName} onclick={(e) => toggle(e, "model")}>
        <span class="lbl">{app.currentModel?.displayName ?? "Select model"}</span>
        {#if comparing.length}<span class="plus" title="Also runs on {comparing.map((id) => app.modelName(id)).join(', ')}">+{comparing.length}</span>{/if}
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
      </button>
      {#if menu === "model"}
        <div class="menu" role="menu" tabindex="-1" style:max-height="{menuMax}px" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
          {#if canCompare}
            <div class="cmp-hint">Add models with <b>Compare</b> to send the same message to each, every one in its own worktree from <code>{app.base}</code>.</div>
          {/if}
          {#each app.models as m (m.id)}
            <div class="mrow">
              <button class="opt" class:sel={m.id === app.model} onclick={() => { app.setModel(m.id); menu = null; }}>
                <div class="name">{m.displayName}</div>
                <div class="desc">{m.description}</div>
                {#if m.id === app.model}<svg class="check" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><path d="m5 12 5 5 9-10" /></svg>{/if}
              </button>
              {#if canCompare && m.id !== app.model}
                <button class="cmp" class:on={app.compare.includes(m.id)} title="Also send to {m.displayName}" onclick={() => app.toggleCompare(m.id)}>
                  {app.compare.includes(m.id) ? "✓ Compare" : "+ Compare"}
                </button>
              {/if}
            </div>
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
      <button class="send" title="Send (Enter)" disabled={(!text.trim() && !files.length) || !app.model} onclick={submit}>
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
  .composer { position: relative; }
  .composer.dropping { border-color: var(--accent); border-style: dashed; }
  .drophint {
    position: absolute; inset: 0; z-index: 5; display: grid; place-items: center; border-radius: inherit; pointer-events: none;
    background: color-mix(in srgb, var(--accent) 10%, var(--panel)); color: var(--accent); font-size: 13px; font-weight: 600;
  }
  .files { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 0 0 8px; }
  .ferr { font-size: 12px; color: var(--danger); }
  .clip { padding: 0 8px; flex: none; }
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
  .mrow { position: relative; }
  .mrow .opt { padding-right: 96px; }
  .mrow .check { right: 10px; }
  .cmp {
    position: absolute; right: 8px; top: 9px; height: 22px; padding: 0 8px; border-radius: 999px;
    font-size: 11px; font-weight: 600; color: var(--faint); border: 1px solid var(--border); background: var(--panel);
  }
  .cmp:hover { color: var(--text); background: var(--hover); }
  .cmp.on { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 50%, var(--border)); }
  .cmp-hint { font-size: 12px; color: var(--muted); padding: 6px 10px 8px; line-height: 1.45; border-bottom: 1px solid var(--border); margin-bottom: 4px; }
  .cmp-hint code { font: 11.5px var(--mono); }
  .plus { flex: none; font-size: 11px; font-weight: 700; color: var(--accent); }
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
