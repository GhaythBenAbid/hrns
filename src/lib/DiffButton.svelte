<script lang="ts">
  import { app, type Pane } from "./store.svelte";
  import { git } from "./gitdiff.svelte";

  let { pane }: { pane: Pane } = $props();

  // Existing chats belong to their folder; a new chat shows the selected project.
  const dir = $derived(pane.current ? pane.current.cwd : app.folder);
  const d = $derived(dir ? git.status[dir] : null);
  const on = $derived(app.diffOpen && app.pane === pane);

  // Opens the sidebar on this pane's project; a second click closes it.
  function toggle() {
    if (on) return app.setDiffOpen(false);
    app.focus(pane);
    app.setDiffOpen(true);
  }
  const fmt = (n: number) => n.toLocaleString("en-US");
</script>

{#if d?.files.length}
  <button
    class="diff-btn"
    class:on
    title="{d.files.length} uncommitted file{d.files.length === 1 ? '' : 's'} — show changes"
    onclick={toggle}
  >
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="2.5" /><circle cx="6" cy="18" r="2.5" /><circle cx="18" cy="8" r="2.5" /><path d="M6 8.5v7M18 10.5c0 4-6 3-11 5.5" /></svg>
    <span class="add">+{fmt(d.add)}</span>
    <span class="del">−{fmt(d.del)}</span>
  </button>
{/if}

<style>
  .diff-btn {
    flex: none; height: 26px; padding: 0 9px; border-radius: 8px; display: inline-flex; align-items: center; gap: 6px;
    border: 1px solid var(--border); background: var(--panel); color: var(--muted);
    font: 600 11.5px var(--mono); font-variant-numeric: tabular-nums; animation: in 0.2s ease-out;
  }
  .diff-btn:hover, .diff-btn.on { background: var(--hover); color: var(--text); border-color: color-mix(in srgb, var(--accent) 35%, var(--border)); }
  .add { color: #4ade80; }
  .del { color: var(--danger); }
  :global([data-theme="light"]) .diff-btn .add { color: #16a34a; }
  @keyframes in { from { opacity: 0; transform: translateY(-3px); } }
</style>
