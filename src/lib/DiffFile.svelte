<script lang="ts">
  import { tick } from "svelte";
  import { diffHtml, fileDiff, reviews, type GitFile, type GitStatus } from "./gitdiff.svelte";

  let { status, file, open = $bindable(), hits = [] }: { status: GitStatus; file: GitFile; open: boolean; hits?: number[] } = $props();

  let el = $state<HTMLElement>();
  let html = $state<string | null>(null);
  let failed = $state(false);

  const cut = $derived(file.path.lastIndexOf("/") + 1);
  const name = $derived(file.path.slice(cut));
  const dir = $derived(file.path.slice(0, Math.max(0, cut - 1)));

  // Review comments: click a line to comment on it; commented lines are marked in the diff.
  const comments = $derived(reviews.of(status.root).filter((c) => c.path === file.path));
  let codeEl = $state<HTMLElement>();
  let editing = $state<{ line: number; side: "new" | "old"; code: string; top: number } | null>(null);
  let draft = $state("");
  let draftBox = $state<HTMLTextAreaElement>();

  async function onCodeClick(e: MouseEvent) {
    if (window.getSelection()?.toString()) return; // selecting text, not commenting
    const row = (e.target as HTMLElement).closest<HTMLElement>(".r");
    if (!row || row.classList.contains("h") || !codeEl) return;
    const n = row.dataset.n, o = row.dataset.o;
    editing = {
      line: Number(n ?? o),
      side: n ? "new" : "old",
      code: row.querySelector("span")?.textContent ?? "",
      top: row.offsetTop + row.offsetHeight,
    };
    draft = "";
    await tick();
    draftBox?.focus();
  }

  function saveDraft() {
    if (!editing || !draft.trim()) return;
    reviews.add(status.root, { path: file.path, line: editing.line, side: editing.side, code: editing.code, text: draft });
    editing = null;
  }

  $effect(() => {
    const el = codeEl, cs = comments;
    html; // re-mark after the diff re-renders
    if (!el) return;
    el.querySelectorAll(".r.cm").forEach((r) => r.classList.remove("cm"));
    for (const c of cs) el.querySelector(`.r[data-${c.side === "new" ? "n" : "o"}="${c.line}"]${c.side === "old" ? ".d" : ""}`)?.classList.add("cm");
  });

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
    {#if comments.length}<span class="ccount" title="{comments.length} review comment{comments.length === 1 ? '' : 's'}">{comments.length}</span>{/if}
    {#if file.binary}<span class="tag">bin</span>{:else}<span class="add">+{file.add}</span><span class="del">−{file.del}</span>{/if}
  </button>
  {#if open}
    <div class="body" style:--rows={Math.min(file.add + file.del + 6, 4000)}>
      {#if file.binary}
        <div class="note">Binary file</div>
      {:else if failed}
        <div class="note">Diff too large to show</div>
      {:else if html !== null}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="code" bind:this={codeEl} onclick={onCodeClick} title="Click a line to comment on it">{@html html}</div>
        {#if editing}
          <div class="editor" style:top="{editing.top}px">
            <div class="where">{file.path}{editing.side === "old" ? `, removed line ${editing.line}` : `:${editing.line}`}</div>
            <textarea
              bind:this={draftBox}
              bind:value={draft}
              rows="2"
              placeholder="What should the agent change here?"
              onkeydown={(e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); saveDraft(); }
                if (e.key === "Escape") { e.stopPropagation(); editing = null; }
              }}
            ></textarea>
            <div class="eactions">
              <span class="kbd">⌘↵</span>
              <button onclick={() => (editing = null)}>Cancel</button>
              <button class="save" disabled={!draft.trim()} onclick={saveDraft}>Add comment</button>
            </div>
          </div>
        {/if}
        {#if comments.length}
          <ul class="comments">
            {#each comments as c (c.id)}
              <li>
                <span class="cl">{c.side === "old" ? `−${c.line}` : c.line}</span>
                <span class="ct">{c.text}</span>
                <button class="cx" title="Remove comment" onclick={() => reviews.remove(status.root, c.id)}>×</button>
              </li>
            {/each}
          </ul>
        {/if}
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

  .body { position: relative; background: var(--code-bg); border-top: 1px solid var(--border); }
  .code :global(.r:not(.h)) { cursor: pointer; }
  .code :global(.r:not(.h):hover) { box-shadow: inset 2px 0 0 var(--accent); }
  .code :global(.r.cm) { box-shadow: inset 3px 0 0 #e0af68; background-image: linear-gradient(rgb(224 175 104 / 0.12), rgb(224 175 104 / 0.12)); }
  .ccount { flex: none; min-width: 16px; height: 16px; padding: 0 4px; border-radius: 8px; background: #e0af68; color: #1a1a1a; font: 700 10px/16px var(--mono); text-align: center; }
  .editor {
    position: absolute; z-index: 2; left: 8px; right: 8px; padding: 8px; border-radius: 10px;
    background: var(--panel); border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--border)); box-shadow: 0 10px 28px rgb(0 0 0 / 0.35);
  }
  .where { font: 11px var(--mono); color: var(--faint); margin-bottom: 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .editor textarea {
    width: 100%; resize: vertical; min-height: 48px; padding: 6px 8px; border-radius: 7px; border: 1px solid var(--border);
    background: var(--bg); color: var(--text); font: 12.5px/1.45 var(--font, inherit); outline: none; user-select: text; -webkit-user-select: text;
  }
  .eactions { display: flex; align-items: center; justify-content: flex-end; gap: 6px; margin-top: 6px; }
  .eactions .kbd { margin-right: auto; font-size: 11px; color: var(--faint); }
  .eactions button { height: 26px; padding: 0 10px; border-radius: 7px; font-size: 12px; font-weight: 550; color: var(--muted); }
  .eactions button:hover:not(:disabled) { background: var(--hover); color: var(--text); }
  .eactions .save { background: var(--text); color: var(--bg); }
  .eactions .save:hover:not(:disabled) { background: var(--text); color: var(--bg); opacity: 0.9; }
  .eactions .save:disabled { opacity: 0.35; }
  .comments { list-style: none; margin: 0; padding: 6px 8px 8px; display: flex; flex-direction: column; gap: 4px; border-top: 1px dashed var(--border); }
  .comments li { display: flex; align-items: baseline; gap: 8px; padding: 5px 8px; border-radius: 7px; background: rgb(224 175 104 / 0.08); font-size: 12px; }
  .cl { flex: none; font: 600 11px var(--mono); color: #e0af68; }
  .ct { flex: 1; min-width: 0; white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; -webkit-user-select: text; }
  .cx { flex: none; width: 18px; height: 18px; border-radius: 5px; color: var(--faint); font-size: 14px; line-height: 1; }
  .cx:hover { background: var(--hover); color: var(--text); }
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
