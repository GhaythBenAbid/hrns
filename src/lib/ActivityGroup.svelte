<script lang="ts">
  import type { Part } from "./store.svelte";
  import { cleanCommand, describe, summarize, type Kind } from "./commands";

  let { parts, live = false }: { parts: Part[]; live?: boolean } = $props();

  let open = $state<boolean | null>(null); // null = automatic (open while live)
  let expanded = $state<Record<string, boolean>>({});

  const isOpen = $derived(open ?? live);
  const sum = $derived(summarize(parts));
  const failed = $derived(parts.filter((p) => p.type === "command" && p.exitCode != null && p.exitCode !== 0).length);
  const visible = $derived(isOpen ? parts : []);

  function duration(ms: number | null | undefined) {
    if (!ms) return "";
    return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)}s`;
  }
  const firstLine = (s: string) => s.replace(/\*\*/g, "").split("\n").find((l) => l.trim()) ?? "";
  const icons: Record<Kind, string> = {
    read: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8ZM14 3v5h5",
    list: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z",
    search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14ZM20 20l-3.5-3.5",
    run: "m5 7 5 5-5 5M12 17h7",
  };
  const verbs: Record<Kind, string> = { read: "Read", list: "Listed", search: "Searched", run: "Ran" };
</script>

<div class="group" class:open={isOpen}>
  <button class="head" onclick={() => (open = !isOpen)}>
    <span class="verb" class:shimmer={live}>{live ? (sum.verb === "Explored" ? "Exploring" : sum.verb === "Thought" ? "Thinking" : "Working") : sum.verb}</span>
    {#if sum.rest}<span class="rest">{sum.rest}</span>{/if}
    {#if failed}<span class="fail">{failed} failed</span>{/if}
    <svg class="chev" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><path d="m9 6 6 6-6 6" /></svg>
  </button>

  {#if visible.length}
    <div class="steps">
      {#each visible as p (p.id)}
        {#if p.type === "reasoning"}
          {#if p.text}
            <button class="step thought" class:exp={expanded[p.id]} onclick={() => (expanded[p.id] = !expanded[p.id])}>
              <span class="ico">
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 3a6 6 0 0 0-4 10.5V17h8v-3.5A6 6 0 0 0 12 3ZM9 21h6" /></svg>
              </span>
              <span class="tline">{expanded[p.id] ? p.text : firstLine(p.text)}</span>
            </button>
          {/if}
        {:else if p.type === "command"}
          {@const rows = describe(p)}
          {@const running = p.status === "inProgress"}
          {@const bad = p.exitCode != null && p.exitCode !== 0}
          <div class="cmd" class:exp={expanded[p.id]}>
            <button class="step" onclick={() => (expanded[p.id] = !expanded[p.id])}>
              <span class="ico" class:bad>
                {#if running}<span class="spin"></span>{:else}
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d={icons[rows[0].kind]} /></svg>
                {/if}
              </span>
              <span class="label">
                {#each rows as r, i}
                  {#if i}<span class="sep">·</span>{/if}
                  <span class="v">{verbs[r.kind]}</span>
                  {#if r.kind === "run"}<code>{r.label}</code>{:else}<span class="target" title={r.detail}>{r.label}</span>{/if}
                  {#if r.detail && r.kind === "search"}<span class="dim">{r.detail}</span>{/if}
                {/each}
              </span>
              <span class="meta">
                {#if p.status === "declined"}<span class="badge bad">declined</span>
                {:else if bad}<span class="badge bad">exit {p.exitCode}</span>
                {:else if !running && p.durationMs}<span class="dim">{duration(p.durationMs)}</span>{/if}
              </span>
            </button>
            {#if expanded[p.id]}
              <div class="term">
                <div class="tbar">
                  <span class="dot r"></span><span class="dot y"></span><span class="dot g"></span>
                  <span class="cwd">{p.cwd?.split("/").filter(Boolean).pop() ?? ""}</span>
                  <button class="copy" onclick={() => navigator.clipboard.writeText(cleanCommand(p.command))}>Copy</button>
                </div>
                <pre class="line"><span class="ps">$</span> {cleanCommand(p.command)}</pre>
                {#if p.output?.trim()}<pre class="out">{p.output.trimEnd()}</pre>
                {:else if !running}<div class="empty">No output</div>{/if}
              </div>
            {/if}
          </div>
        {:else if p.type === "tool"}
          <div class="step static">
            <span class="ico" class:bad={!!p.error}>
              {#if p.status === "inProgress"}<span class="spin"></span>{:else}
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.6 2.6-2.4-.6-.6-2.4Z" /></svg>
              {/if}
            </span>
            <span class="label"><span class="target">{p.title}</span></span>
            {#if p.error}<span class="meta"><span class="badge bad">failed</span></span>{/if}
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .group { margin: 6px 0 14px; min-width: 0; max-width: 100%; }
  .head {
    min-width: 0; display: inline-flex; align-items: center; gap: 8px; padding: 4px 10px 4px 0;
    font-size: 13.5px; color: var(--muted); max-width: 100%;
  }
  .head:hover { color: var(--text); }
  .verb { font-weight: 600; flex: none; }
  .rest { color: var(--faint); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .head:hover .rest { color: var(--muted); }
  .fail { color: var(--danger); font-size: 12px; flex: none; }
  .chev { flex: none; color: var(--faint); transition: rotate 0.15s; }
  .open .chev { rotate: 90deg; }

  .steps {
    margin: 6px 0 0 5px; padding: 2px 0 2px 14px; border-left: 1px solid var(--border);
    display: grid; grid-template-columns: minmax(0, 1fr); gap: 1px; min-width: 0;
  }
  .cmd { min-width: 0; }
  .step {
    width: 100%; display: flex; align-items: center; gap: 10px; padding: 5px 8px; border-radius: 8px;
    text-align: left; font-size: 13px; color: var(--muted); min-width: 0;
  }
  button.step:hover { background: var(--hover); color: var(--text); }
  .ico { flex: none; width: 16px; display: grid; place-items: center; color: var(--faint); }
  .ico.bad { color: var(--danger); }
  .label { flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 6px; overflow: hidden; white-space: nowrap; }
  .v { flex: none; color: var(--faint); }
  .target { color: var(--text); min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .label code {
    flex: 0 1 auto; min-width: 0; font: 12.5px var(--mono); color: var(--text);
    overflow: hidden; white-space: nowrap; text-overflow: ellipsis;
  }
  .sep { color: var(--faint); }
  .dim { color: var(--faint); font-size: 12px; flex: none; }
  .meta { flex: none; display: flex; gap: 6px; align-items: center; }
  .badge { font-size: 11px; font-weight: 600; padding: 1px 7px; border-radius: 999px; }
  .badge.bad { color: var(--danger); background: var(--danger-bg); }

  .thought .tline { flex: 1; min-width: 0; font-size: 13px; font-style: italic; color: var(--faint); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .thought.exp { align-items: flex-start; }
  .thought.exp .tline { white-space: pre-wrap; font-style: normal; color: var(--muted); user-select: text; -webkit-user-select: text; }

  .term {
    margin: 4px 0 8px 26px; min-width: 0; border: 1px solid var(--border); border-radius: 10px; overflow: hidden;
    background: var(--code-bg); user-select: text; -webkit-user-select: text;
  }
  .tbar {
    display: flex; align-items: center; gap: 6px; padding: 7px 10px;
    background: var(--raised); border-bottom: 1px solid var(--border);
  }
  .tbar .dot { width: 9px; height: 9px; border-radius: 50%; background: var(--border); }
  .cwd { margin-left: 6px; font: 11.5px var(--mono); color: var(--faint); flex: 1; }
  .copy { font-size: 11.5px; color: var(--faint); padding: 1px 8px; border-radius: 6px; }
  .copy:hover { color: var(--text); background: var(--hover); }
  .term pre { margin: 0; padding: 8px 12px; font: 12px/1.6 var(--mono); white-space: pre-wrap; overflow-wrap: anywhere; }
  .line { color: var(--text); }
  .ps { color: var(--accent); user-select: none; -webkit-user-select: none; }
  .out { color: var(--muted); max-height: 300px; overflow: auto; border-top: 1px dashed var(--border); }
  .empty { padding: 6px 12px 10px; font: 12px var(--mono); color: var(--faint); }

  .spin {
    width: 10px; height: 10px; border-radius: 50%;
    border: 1.6px solid color-mix(in srgb, var(--accent) 30%, transparent); border-top-color: var(--accent);
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin { to { rotate: 360deg; } }
  .shimmer {
    background: linear-gradient(90deg, var(--faint) 0%, var(--text) 50%, var(--faint) 100%) 0 0 / 200% 100%;
    -webkit-background-clip: text; background-clip: text; color: transparent;
    animation: shimmer 1.6s linear infinite;
  }
  @keyframes shimmer { to { background-position: -200% 0; } }
</style>
