<script lang="ts">
  import { diffHtml, fileDiff, type GitFile, type GitStatus } from "./gitdiff.svelte";

  let { status, file, open = $bindable(), hits = [] }: { status: GitStatus; file: GitFile; open: boolean; hits?: number[] } = $props();

  let el = $state<HTMLElement>();
  let html = $state<string | null>(null);
  let failed = $state(false);

  const cut = $derived(file.path.lastIndexOf("/") + 1);
  const name = $derived(file.path.slice(cut));
  const dir = $derived(file.path.slice(0, Math.max(0, cut - 1)));

  /** Text split into runs, marking the characters a fuzzy search matched. */
  function runs(text: string, from: number) {
    const hit = new Set(hits.map((p) => p - from));
    const out: { t: string; on: boolean }[] = [];
    for (let i = 0; i < text.length; i++) {
      const on = hit.has(i);
      if (out.length && out[out.length - 1].on === on) out[out.length - 1].t += text[i];
      else out.push({ t: text[i], on });
    }
    return out;
  }

  // Fetch and build the patch only when it's open and near the viewport.
  $effect(() => {
    const s = status, f = file;
    html = null;
    failed = false;
    if (!open || !el || f.binary) return;
    let live = true;
    const io = new IntersectionObserver(
      ([e]) => {
        if (!e.isIntersecting) return;
        io.disconnect();
        fileDiff(s, f).then((patch) => {
          if (!live) return;
          if (patch == null) failed = true;
          else html = diffHtml(patch);
        });
      },
      { rootMargin: "600px 0px" },
    );
    io.observe(el);
    return () => {
      live = false;
      io.disconnect();
    };
  });
</script>

<section class="file" bind:this={el} data-path={file.path}>
  <button class="head" title={file.old ? `${file.old} → ${file.path}` : file.path} onclick={() => (open = !open)}>
    <svg class="chev" class:closed={!open} width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.2" stroke-linecap="round"><path d="m6 9 6 6 6-6" /></svg>
    <span class="dot {file.kind}"></span>
    <span class="name">{#each runs(name, cut) as r}<span class:hl={r.on}>{r.t}</span>{/each}</span>
    <span class="dir">{#each runs(dir, 0) as r}<span class:hl={r.on}>{r.t}</span>{/each}</span>
    {#if file.binary}<span class="tag">bin</span>{:else}<span class="add">+{file.add}</span><span class="del">−{file.del}</span>{/if}
  </button>
  {#if open}
    <div class="body" style:--rows={Math.min(file.add + file.del + 6, 4000)}>
      {#if file.binary}
        <div class="note">Binary file</div>
      {:else if failed}
        <div class="note">Diff too large to show</div>
      {:else if html !== null}
        <div class="code">{@html html}</div>
      {:else}
        <div class="loading"></div>
      {/if}
    </div>
  {/if}
</section>

<style>
  /* Off-screen files skip layout and paint entirely. */
  .file { content-visibility: auto; contain-intrinsic-size: auto 34px; border-bottom: 1px solid var(--border); }
  .head {
    position: sticky; top: 0; z-index: 1; width: 100%; height: 34px; display: flex; align-items: center; gap: 7px;
    padding: 0 12px; background: var(--sidebar); text-align: left; font-size: 12.5px; white-space: nowrap;
  }
  .head:hover { background: var(--hover); }
  .chev { flex: none; color: var(--faint); transition: rotate 0.12s; }
  .chev.closed { rotate: -90deg; }
  .dot { flex: none; width: 6px; height: 6px; border-radius: 50%; background: #e0af68; }
  .dot.add { background: #4ade80; }
  .dot.delete { background: var(--danger); }
  .dot.rename { background: #7aa2f7; }
  .name { flex: none; max-width: 60%; overflow: hidden; text-overflow: ellipsis; font-weight: 550; }
  .dir { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; color: var(--faint); font-size: 11.5px; }
  .hl { color: var(--accent); font-weight: 650; }
  .add, .del, .tag { flex: none; font: 600 11px var(--mono); font-variant-numeric: tabular-nums; }
  .add { color: #4ade80; }
  .del { color: var(--danger); }
  .tag { color: var(--faint); }
  :global([data-theme="light"]) .file .add { color: #16a34a; }

  .body { background: var(--code-bg); border-top: 1px solid var(--border); }
  .code { overflow-x: auto; padding: 4px 0; font: 11.5px/1.65 var(--mono); user-select: text; -webkit-user-select: text; }
  .code :global(.r) { display: flex; min-width: max-content; }
  .code :global(.r i) {
    flex: none; width: 34px; padding-right: 6px; text-align: right; font-style: normal; color: var(--faint); opacity: 0.6;
    user-select: none; -webkit-user-select: none;
  }
  .code :global(.r b) { flex: none; width: 14px; text-align: center; font-weight: 400; user-select: none; -webkit-user-select: none; }
  .code :global(.r span) { white-space: pre; padding-right: 14px; }
  .code :global(.r.a) { background: rgb(74 222 128 / 0.1); color: #4ade80; }
  .code :global(.r.d) { background: rgb(248 113 113 / 0.1); color: var(--danger); }
  :global([data-theme="light"]) .code :global(.r.a) { color: #15803d; }
  .code :global(.r.h) { color: color-mix(in srgb, var(--accent) 75%, var(--muted)); background: color-mix(in srgb, var(--accent) 7%, transparent); }
  .note { padding: 10px 14px; font-size: 12px; color: var(--faint); }
  /* Reserve roughly the diff's height while it loads, so the list doesn't jump. */
  .loading { height: calc(min(var(--rows), 40) * 19px); background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--text) 4%, transparent), transparent) 0 0 / 200% 100%; animation: sheen 1.1s linear infinite; }
  @keyframes sheen { to { background-position: -200% 0; } }
</style>
