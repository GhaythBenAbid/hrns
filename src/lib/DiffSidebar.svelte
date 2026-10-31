<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { app } from "./store.svelte";
  import { git, refreshDiff } from "./gitdiff.svelte";
  import DiffFile from "./DiffFile.svelte";
  import { fuzzy } from "./fuzzy";

  let { dir }: { dir: string | null | undefined } = $props();

  const s = $derived(dir ? git.status[dir] : undefined);
  let query = $state("");
  let search = $state<HTMLInputElement>();
  // fzf-style filter over paths, best match first; matched characters are highlighted in each row.
  const files = $derived.by(() => {
    const all = s?.files ?? [];
    const q = query.trim();
    if (!q) return all.map((f) => ({ f, hits: [] as number[] }));
    const out = [];
    for (const f of all) {
      const m = fuzzy(q, f.path);
      if (m) out.push({ f, hits: m.positions, score: m.score });
    }
    return out.sort((a, b) => b.score - a.score || a.f.path.length - b.f.path.length);
  });
  // Small files start open; big ones wait for a click.
  let opened = $state<Record<string, boolean>>({});
  const isOpen = (f: { path: string; add: number; del: number }, i: number) => opened[f.path] ?? (i < 12 && f.add + f.del <= 300);
  let branch = $state<string | null>(null);
  let spinning = $state(false);
  const fmt = (n: number) => n.toLocaleString("en-US");

  $effect(() => {
    const d = dir;
    branch = null;
    if (!d) return;
    refreshDiff(d);
    invoke<string | null>("git_branch", { path: d }).then((b) => { if (dir === d) branch = b; });
  });

  async function reload() {
    spinning = true;
    await refreshDiff(dir);
    setTimeout(() => (spinning = false), 350);
  }

  function setAll(open: boolean) {
    opened = { ...opened, ...Object.fromEntries(files.map(({ f }) => [f.path, open])) };
  }

  // Drag the left edge to resize (pointer capture: no window listeners, no layout thrash between frames).
  function resize(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const right = el.parentElement!.getBoundingClientRect().right;
    let raf = 0;
    const move = (ev: PointerEvent) => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => app.setDiffWidth(right - ev.clientX));
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("lostpointercapture", () => el.removeEventListener("pointermove", move), { once: true });
  }
</script>

<aside class="diffs" style:width="{app.diffWidth}px">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grip" title="Drag to resize" onpointerdown={resize}></div>
  <header>
    <div class="title">
      <span>Changes</span>
      {#if branch}<span class="branch">{branch}</span>{/if}
    </div>
    <button class="icon-btn" title="Collapse all" onclick={() => setAll(false)}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="m7 20 5-5 5 5M7 4l5 5 5-5" /></svg>
    </button>
    <button class="icon-btn" title="Expand all" onclick={() => setAll(true)}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="m7 15 5 5 5-5M7 9l5-5 5 5" /></svg>
    </button>
    <button class="icon-btn" class:spinning title="Refresh" onclick={reload}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-2.6-6.4M21 4v5h-5" /></svg>
    </button>
    <button class="icon-btn" title="Close (Esc)" onclick={() => app.setDiffOpen(false)}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m6 6 12 12M18 6 6 18" /></svg>
    </button>
  </header>

  {#if s}
    <div class="summary">
      {s.files.length} file{s.files.length === 1 ? "" : "s"} changed
      <span class="add">+{fmt(s.add)}</span><span class="del">−{fmt(s.del)}</span>
    </div>
  {/if}

  {#if s?.files.length}
    <label class="search">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></svg>
      <input
        bind:this={search}
        bind:value={query}
        placeholder="Filter files"
        spellcheck="false"
        autocomplete="off"
        onkeydown={(e) => { if (e.key === "Escape" && query) { e.stopPropagation(); query = ""; } }}
      />
      {#if query}<span class="count">{files.length}/{s.files.length}</span>{/if}
    </label>
  {/if}

  <div class="list">
    {#if s}
      {#each files as { f, hits }, i (f.path)}
        <DiffFile status={s} file={f} {hits} bind:open={() => isOpen(f, i), (v) => (opened[f.path] = v)} />
      {:else}
        <div class="empty">{query ? `No files match “${query}”` : "No uncommitted changes"}</div>
      {/each}
    {:else if s === null}
      <div class="empty">{dir ? "Not a git repository" : "No project selected"}</div>
    {:else}
      <div class="empty">Reading changes…</div>
    {/if}
  </div>
</aside>

<style>
  .diffs {
    position: relative; flex: none; min-width: 0; height: 100%; display: flex; flex-direction: column;
    background: var(--sidebar); border-left: 1px solid var(--border); animation: in 0.22s cubic-bezier(0.32, 0.72, 0, 1);
  }
  @keyframes in { from { opacity: 0; transform: translateX(24px); } }
  @media (prefers-reduced-motion: reduce) { .diffs { animation: none; } }
  .grip { position: absolute; z-index: 3; left: -4px; top: 0; bottom: 0; width: 8px; cursor: col-resize; }
  .grip:hover, .grip:active { background: linear-gradient(90deg, transparent 3px, color-mix(in srgb, var(--accent) 60%, transparent) 3px 5px, transparent 5px); }

  header { flex: none; height: 44px; display: flex; align-items: center; gap: 0; padding: 0 6px 0 14px; border-bottom: 1px solid var(--border); }
  .title { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 600; }
  .branch { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: 500 11px var(--mono); color: var(--muted); background: var(--raised); padding: 1px 7px; border-radius: 6px; }
  header .icon-btn { width: 28px; height: 28px; }
  .spinning svg { animation: spin 0.6s linear; }
  @keyframes spin { to { rotate: 360deg; } }

  .summary { flex: none; display: flex; align-items: baseline; gap: 8px; padding: 8px 14px; font-size: 12px; color: var(--muted); border-bottom: 1px solid var(--border); }
  .add, .del { font: 600 11.5px var(--mono); font-variant-numeric: tabular-nums; }
  .add { color: #4ade80; }
  .del { color: var(--danger); }
  :global([data-theme="light"]) .summary .add { color: #16a34a; }

  .search {
    flex: none; display: flex; align-items: center; gap: 7px; height: 32px; margin: 8px 10px 6px; padding: 0 10px;
    border-radius: 8px; background: color-mix(in srgb, var(--raised) 60%, transparent); color: var(--faint);
  }
  .search:focus-within { background: var(--raised); color: var(--muted); }
  .search input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font-size: 12.5px; }
  .search input::placeholder { color: var(--faint); }
  .count { font: 11px var(--mono); color: var(--faint); font-variant-numeric: tabular-nums; }
  .list { flex: 1; min-height: 0; overflow-y: auto; overflow-x: hidden; overscroll-behavior: contain; }
  .empty { padding: 32px 16px; text-align: center; font-size: 12.5px; color: var(--faint); }
</style>
