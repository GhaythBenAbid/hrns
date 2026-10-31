<script lang="ts">
  import { app, type Approval } from "./store.svelte";

  let { approval }: { approval: Approval } = $props();

  const title = $derived(
    approval.kind === "command" ? "Allow Codex to run this command?"
    : approval.kind === "fileChange" ? "Allow Codex to make these changes?"
    : "Allow Codex extra permissions?",
  );
</script>

<div class="card">
  <div class="title">{title}</div>
  {#if approval.reason}<div class="reason">{approval.reason}</div>{/if}

  {#if approval.command}
    <pre class="cmd">$ {approval.command}</pre>
    {#if approval.cwd}<div class="cwd">in {approval.cwd}</div>{/if}
  {/if}
  {#if approval.changes?.length}
    <ul class="files">
      {#each approval.changes as c}<li><span>{c.kind}</span>{c.path}</li>{/each}
    </ul>
  {/if}
  {#if approval.grantRoot}<div class="cwd">Write access to {approval.grantRoot}</div>{/if}

  <div class="actions">
    <button class="primary" onclick={() => app.respond(approval.key, "accept")}>Approve <kbd>⌘↵</kbd></button>
    <button onclick={() => app.respond(approval.key, "acceptForSession")}>Approve for this chat</button>
    <div class="spacer"></div>
    <button class="deny" onclick={() => app.respond(approval.key, "decline")}>Deny</button>
  </div>
</div>

<style>
  .card {
    margin: 10px 0 16px; padding: 14px 16px; border-radius: 14px;
    border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border));
    background: color-mix(in srgb, var(--accent) 6%, var(--panel));
  }
  .title { font-weight: 600; font-size: 14px; }
  .reason { color: var(--muted); font-size: 13px; margin-top: 4px; }
  .cmd {
    margin: 10px 0 0; padding: 10px 12px; border-radius: 10px; background: var(--code-bg); border: 1px solid var(--border);
    font: 12.5px/1.5 var(--mono); white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; -webkit-user-select: text;
  }
  .cwd { color: var(--faint); font-size: 12px; margin-top: 6px; font-family: var(--mono); }
  .files { margin: 10px 0 0; padding: 0; list-style: none; font: 12.5px var(--mono); }
  .files li { padding: 2px 0; }
  .files span { display: inline-block; min-width: 56px; color: var(--muted); text-transform: capitalize; }
  .actions { display: flex; gap: 8px; margin-top: 14px; align-items: center; }
  .spacer { flex: 1; }
  .actions button { height: 32px; padding: 0 14px; border-radius: 9px; border: 1px solid var(--border); font-size: 13px; font-weight: 500; }
  .actions button:hover { background: var(--hover); }
  .actions .primary { background: var(--text); color: var(--bg); border-color: transparent; }
  .actions .primary:hover { background: var(--text); opacity: 0.9; }
  .actions .deny:hover { color: var(--danger); }
  kbd { font: 11px var(--font); opacity: 0.6; margin-left: 4px; }
</style>
