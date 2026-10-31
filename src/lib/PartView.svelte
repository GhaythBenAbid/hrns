<script lang="ts">
  import { render } from "./markdown";
  import type { Part } from "./store.svelte";

  let { part, live = false }: { part: Part; live?: boolean } = $props();

  const html = $derived(part.type === "text" && part.text ? render(part.text) : "");
  const running = $derived("status" in part && part.status === "inProgress");

  // Codex sends unified diffs for edits, but the raw file contents for adds/deletes.
  function diffLines(c: { kind: string; diff: string }): string[] {
    const lines = c.diff.replace(/\n$/, "").split("\n");
    if (c.kind === "add") return lines.map((l) => "+" + l);
    if (c.kind === "delete") return lines.map((l) => "-" + l);
    return lines;
  }

  function stat(lines: string[]) {
    let add = 0, del = 0;
    for (const l of lines) {
      if (l.startsWith("+") && !l.startsWith("+++")) add++;
      else if (l.startsWith("-") && !l.startsWith("---")) del++;
    }
    return { add, del };
  }
  const base = (p: string) => p.split("/").pop() ?? p;
</script>

{#if part.type === "text"}
  {#if html}<div class="md text">{@html html}</div>{/if}
{:else if part.type === "plan"}
  {#if part.text}<div class="plan"><div class="label">Plan</div><div class="md">{@html render(part.text)}</div></div>{/if}
{:else if part.type === "reasoning"}
  {#if part.text}
    <details class="step" open={live}>
      <summary><span class="verb">{live ? "Thinking" : "Thought"}</span></summary>
      <div class="rtext">{part.text}</div>
    </details>
  {/if}
{:else if part.type === "command"}
  <details class="step">
    <summary>
      <span class="verb" class:shimmer={running}>{running ? "Running" : part.status === "declined" ? "Declined" : "Ran"}</span>
      <code class="cmd">{part.command}</code>
      {#if part.exitCode != null && part.exitCode !== 0}<span class="bad">exit {part.exitCode}</span>{/if}
    </summary>
    <div class="out">
      <div class="prompt">$ {part.command}</div>
      {#if part.output}<pre>{part.output}</pre>{:else if !running}<div class="none">No output</div>{/if}
    </div>
  </details>
{:else if part.type === "fileChange"}
  {#each part.changes as c (c.path)}
    {@const lines = diffLines(c)}
    {@const s = stat(lines)}
    <details class="step">
      <summary>
        <span class="verb" class:shimmer={running}>{c.kind === "add" ? "Created" : c.kind === "delete" ? "Deleted" : "Edited"}</span>
        <code class="cmd" title={c.path}>{base(c.path)}</code>
        <span class="add">+{s.add}</span><span class="del">-{s.del}</span>
        {#if part.status === "declined"}<span class="bad">declined</span>{/if}
      </summary>
      <pre class="diff">{#each lines as l}<span class:a={l.startsWith("+")} class:d={l.startsWith("-")} class:h={l.startsWith("@@")}>{l}
</span>{/each}</pre>
    </details>
  {/each}
{:else if part.type === "tool"}
  <div class="step flat">
    <span class="verb" class:shimmer={running}>{running ? "Calling" : "Called"}</span>
    <code class="cmd">{part.title}</code>
    {#if part.error}<span class="bad">{part.error}</span>{/if}
  </div>
{/if}

<style>
  .text { margin: 4px 0 14px; }
  .step { margin: 2px 0; font-size: 13.5px; color: var(--muted); min-width: 0; max-width: 100%; }
  .step summary, .flat { display: flex; align-items: center; gap: 8px; list-style: none; padding: 3px 0; min-width: 0; }
  .step summary::-webkit-details-marker { display: none; }
  .step summary:hover .verb { color: var(--text); }
  .verb { flex: none; font-weight: 550; }
  .cmd {
    flex: 0 1 auto; font: 12.5px var(--mono); color: var(--text); opacity: 0.85;
    overflow: hidden; white-space: nowrap; text-overflow: ellipsis; min-width: 0;
  }
  .add { color: #4ade80; font: 12px var(--mono); flex: none; }
  .del { color: var(--danger); font: 12px var(--mono); flex: none; }
  .bad { color: var(--danger); font-size: 12px; flex: none; }
  .shimmer {
    background: linear-gradient(90deg, var(--faint) 0%, var(--text) 50%, var(--faint) 100%) 0 0 / 200% 100%;
    -webkit-background-clip: text; background-clip: text; color: transparent;
    animation: shimmer 1.6s linear infinite;
  }
  @keyframes shimmer { to { background-position: -200% 0; } }
  .rtext { margin: 4px 0 10px; padding-left: 12px; border-left: 2px solid var(--border); white-space: pre-wrap; user-select: text; -webkit-user-select: text; }
  .out, .diff {
    margin: 6px 0 10px; border: 1px solid var(--border); border-radius: 10px; background: var(--code-bg);
    font: 12px/1.55 var(--mono); max-height: 320px; overflow: auto; user-select: text; -webkit-user-select: text;
  }
  .out { padding: 10px 12px; }
  .prompt { color: var(--text); margin-bottom: 6px; white-space: pre-wrap; }
  .out pre { margin: 0; white-space: pre-wrap; color: var(--muted); }
  .none { color: var(--faint); }
  .diff { padding: 8px 0; margin-top: 6px; }
  .diff span { display: block; padding: 0 12px; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--muted); }
  .diff .a { color: #4ade80; background: rgb(74 222 128 / 0.08); }
  .diff .d { color: var(--danger); background: rgb(248 113 113 / 0.08); }
  .diff .h { color: #7aa2f7; }
  .plan { margin: 6px 0 14px; padding: 10px 14px; border: 1px solid var(--border); border-radius: 12px; }
  .label { font-size: 12px; font-weight: 600; color: var(--muted); margin-bottom: 4px; }
</style>
