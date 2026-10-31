<script lang="ts">
  import { app, type Pane } from "./store.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";

  let { pane, autofocus = false }: { pane: Pane; autofocus?: boolean } = $props();
  let text = $state("");
  let box: HTMLTextAreaElement;
  let menu = $state<"model" | "effort" | "access" | null>(null);

  const accessModes = [
    { id: "read-only", label: "Read only", desc: "Codex can read files. Asks before editing or running commands." },
    { id: "auto", label: "Auto", desc: "Edits files and runs commands in the project. Asks for anything outside it or for network." },
    { id: "full", label: "Full access", desc: "Can edit anything and use the network without asking. Use with care." },
  ] as const;
  const accessLabel = $derived(accessModes.find((m) => m.id === app.access)?.label ?? "Auto");
  // Existing chats keep their folder; new chats use the selected one.
  const folder = $derived(pane.current ? pane.current.cwd : app.folder);
  const folderName = $derived(folder ? folder.split("/").filter(Boolean).pop() : null);

  const streaming = $derived(!!pane.current && !!app.streaming[pane.current.id]);
  const efforts = $derived(app.currentModel?.supportedReasoningEfforts ?? []);
  const effortLabel = $derived(app.effort ?? app.currentModel?.defaultReasoningEffort ?? "default");

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
      <button class="pill" title={app.currentModel?.displayName} onclick={(e) => { e.stopPropagation(); menu = menu === "model" ? null : "model"; }}>
        <span class="lbl">{app.currentModel?.displayName ?? "Select model"}</span>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
      </button>
      {#if menu === "model"}
        <div class="menu" role="menu" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
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
        <button class="pill" title="Reasoning: {effortLabel}" onclick={(e) => { e.stopPropagation(); menu = menu === "effort" ? null : "effort"; }}>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 3a6 6 0 0 0-4 10.5V17h8v-3.5A6 6 0 0 0 12 3ZM9 21h6" /></svg>
          <span class="cap lbl hide-sm">{effortLabel}</span>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
        </button>
        {#if menu === "effort"}
          <div class="menu small" role="menu" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
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
      <button class="pill" class:full={app.access === "full"} title="Access: {accessLabel}" onclick={(e) => { e.stopPropagation(); menu = menu === "access" ? null : "access"; }}>
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 4 6v6c0 4.5 3.4 8.2 8 9 4.6-.8 8-4.5 8-9V6Z" /></svg>
        <span class="lbl hide-sm">{accessLabel}</span>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
      </button>
      {#if menu === "access"}
        <div class="menu" role="menu" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
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
