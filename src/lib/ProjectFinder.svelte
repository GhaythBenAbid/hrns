<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { app } from "./store.svelte";
  import { fuzzy } from "./fuzzy";
  import ProjectIcon from "./ProjectIcon.svelte";

  const LIMIT = 200;

  let query = $state("");
  let sel = $state(0);
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLElement>();

  // Folders with chats come first (most recent first), then everything found on disk.
  const candidates = $derived.by(() => {
    const seen = new Set<string>();
    const out: { path: string; recent: boolean; modified: number }[] = [];
    const recent = [app.folder, ...app.threads.map((t) => t.cwd)];
    for (const p of recent) {
      if (!p || seen.has(p)) continue;
      seen.add(p);
      out.push({ path: p, recent: true, modified: Infinity });
    }
    for (const p of app.projects) {
      if (seen.has(p.path)) continue;
      seen.add(p.path);
      out.push({ path: p.path, recent: false, modified: p.modified });
    }
    return out;
  });

  const results = $derived.by(() => {
    const q = query.trim();
    if (!q) return candidates.slice(0, LIMIT).map((c) => ({ ...c, positions: [] as number[], score: 0 }));
    const out = [];
    for (const c of candidates) {
      const m = fuzzy(q, c.path);
      if (m) out.push({ ...c, ...m });
    }
    // Same tiebreaks as fzf (score, then shorter), with folders you've used winning ties.
    out.sort((a, b) => b.score - a.score || Number(b.recent) - Number(a.recent) || a.path.length - b.path.length);
    return out.slice(0, LIMIT);
  });

  $effect(() => {
    query;
    sel = 0;
  });
  $effect(() => {
    list?.querySelector(`[data-i="${sel}"]`)?.scrollIntoView({ block: "nearest" });
  });

  const home = (p: string) => p.match(/^\/Users\/[^/]+/)?.[0].length ?? 0;

  /** Text split into plain and matched runs, for highlighting. */
  function runs(text: string, from: number, positions: number[]) {
    const hit = new Set(positions.map((p) => p - from));
    const out: { t: string; on: boolean }[] = [];
    for (let i = 0; i < text.length; i++) {
      const on = hit.has(i);
      if (out.length && out[out.length - 1].on === on) out[out.length - 1].t += text[i];
      else out.push({ t: text[i], on });
    }
    return out;
  }

  function choose(path: string | undefined) {
    if (path) app.finder?.resolve(path);
  }

  async function browse() {
    const finder = app.finder;
    const dir = await app.browseFolder();
    if (dir) finder?.resolve(dir);
    else input?.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    const ctrl = e.ctrlKey && !e.metaKey;
    if (e.key === "ArrowDown" || (ctrl && (e.key === "n" || e.key === "j"))) {
      e.preventDefault();
      sel = Math.min(sel + 1, results.length - 1);
    } else if (e.key === "ArrowUp" || (ctrl && (e.key === "p" || e.key === "k"))) {
      e.preventDefault();
      sel = Math.max(sel - 1, 0);
    } else if (e.key === "Enter") {
      e.preventDefault();
      choose(results[sel]?.path);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      app.finder?.resolve(null);
    } else if (e.key === "o" && e.metaKey) {
      e.preventDefault();
      browse();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" transition:fade={{ duration: 120 }} onclick={() => app.finder?.resolve(null)}>
  <div class="finder" role="dialog" tabindex="-1" aria-label="Open project" transition:scale={{ start: 0.97, duration: 160 }} onclick={(e) => e.stopPropagation()}>
    <div class="prompt">
      <span class="caret" aria-hidden="true">›</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:this={input} bind:value={query} {onkeydown} placeholder="Find a project…" spellcheck="false" autocomplete="off" autofocus />
      <span class="count">
        {#if app.scanning && !app.projects.length}<span class="spin"></span>{/if}
        {results.length}{results.length === LIMIT ? "+" : ""}/{candidates.length}
      </span>
    </div>

    <div class="list" bind:this={list} role="listbox">
      {#each results as r, i (r.path)}
        {@const base = r.path.lastIndexOf("/") + 1}
        {@const h = home(r.path)}
        <button class="item" class:sel={i === sel} data-i={i} role="option" aria-selected={i === sel} onmousemove={() => (sel = i)} onclick={() => choose(r.path)}>
          <ProjectIcon path={r.path} size={20} />
          <span class="name">{#each runs(r.path.slice(base), base, r.positions) as s}<span class:hl={s.on}>{s.t}</span>{/each}</span>
          <span class="dir">{#if h}~{/if}{#each runs(r.path.slice(h, base - 1), h, r.positions) as s}<span class:hl={s.on}>{s.t}</span>{/each}</span>
          {#if r.recent}<span class="tag">recent</span>{/if}
        </button>
      {:else}
        <div class="empty">
          {#if app.scanning}Looking for projects…{:else if query}No projects match “{query}”{:else}No projects found{/if}
        </div>
      {/each}
    </div>

    <div class="foot">
      <span><kbd>↑</kbd><kbd>↓</kbd> move</span>
      <span><kbd>↵</kbd> open</span>
      <span><kbd>esc</kbd> close</span>
      <button class="browse" onclick={browse}>Browse… <kbd>⌘O</kbd></button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 300; display: flex; justify-content: center; align-items: flex-start;
    padding: 12vh 16px 16px; background: rgb(0 0 0 / 0.35);
  }
  .finder {
    width: min(640px, 100%); max-height: min(520px, 76vh); display: flex; flex-direction: column; overflow: hidden;
    background: var(--panel); border: 1px solid var(--border); border-radius: 16px;
    box-shadow: 0 24px 70px rgb(0 0 0 / 0.5);
  }
  .prompt { display: flex; align-items: center; gap: 10px; padding: 0 16px; height: 52px; border-bottom: 1px solid var(--border); flex: none; }
  .caret { font: 600 20px var(--mono); color: var(--accent); line-height: 1; margin-top: -2px; }
  .prompt input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; font-size: 15px; }
  .prompt input::placeholder { color: var(--faint); }
  .count { display: flex; align-items: center; gap: 6px; font: 11.5px var(--mono); color: var(--faint); font-variant-numeric: tabular-nums; }
  .spin {
    width: 10px; height: 10px; border-radius: 50%;
    border: 1.6px solid color-mix(in srgb, var(--accent) 30%, transparent); border-top-color: var(--accent);
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin { to { rotate: 360deg; } }

  .list { flex: 1; overflow-y: auto; padding: 6px; }
  .item {
    width: 100%; display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-radius: 9px; text-align: left;
    font-size: 13.5px; white-space: nowrap;
  }
  .item.sel { background: var(--hover); box-shadow: inset 2px 0 0 var(--accent); }
  .name { flex: none; max-width: 50%; overflow: hidden; text-overflow: ellipsis; font-weight: 550; }
  .dir { flex: 1; align-self: baseline; margin-top: 1px; min-width: 0; overflow: hidden; text-overflow: ellipsis; font-size: 12px; color: var(--faint); }
  .hl { color: var(--accent); font-weight: 650; }
  .dir .hl { font-weight: 600; }
  .tag {
    flex: none; align-self: center; font-size: 10.5px; font-weight: 600; padding: 1px 7px; border-radius: 999px;
    color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .empty { padding: 28px 12px; text-align: center; color: var(--faint); font-size: 13px; }

  .foot {
    flex: none; display: flex; align-items: center; gap: 14px; padding: 8px 12px 8px 16px;
    border-top: 1px solid var(--border); font-size: 11.5px; color: var(--faint);
  }
  kbd {
    font: 10.5px var(--font); color: var(--muted); background: var(--raised); border: 1px solid var(--border);
    padding: 0 5px; border-radius: 5px; margin-right: 3px;
  }
  .browse { margin-left: auto; display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-radius: 7px; font-size: 12px; color: var(--muted); }
  .browse:hover { background: var(--hover); color: var(--text); }
  .browse kbd { margin: 0; }
</style>
