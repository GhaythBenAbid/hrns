<script lang="ts">
  import PartView from "./PartView.svelte";
  import ActivityGroup from "./ActivityGroup.svelte";
  import ApprovalCard from "./ApprovalCard.svelte";
  import AttachmentChip from "./AttachmentChip.svelte";
  import { toAttachment } from "./attachments";
  import type { Approval, Message, Part } from "./store.svelte";

  let {
    msg,
    live = false,
    approvals = [],
    notice,
  }: { msg: Pick<Message, "role" | "text" | "parts" | "model" | "error" | "attachments">; live?: boolean; approvals?: Approval[]; notice?: string } = $props();

  // Older messages (or plain chat) may have only text.
  const parts = $derived<Part[]>(msg.parts ?? (msg.text ? [{ type: "text", id: "t", text: msg.text }] : []));
  // Runs of thinking / commands / tool calls collapse into one activity group, like Codex.
  type Block = { kind: "part"; part: Part } | { kind: "group"; id: string; parts: Part[] };
  const blocks = $derived.by(() => {
    const out: Block[] = [];
    for (const p of parts) {
      if (p.type === "command" || p.type === "tool" || p.type === "reasoning") {
        const last = out[out.length - 1];
        if (last?.kind === "group") last.parts.push(p);
        else out.push({ kind: "group", id: p.id, parts: [p] });
      } else if (p.type !== "text" || p.text) {
        out.push({ kind: "part", part: p });
      }
    }
    return out;
  });

  const busy = $derived(live && !approvals.length && blocks.at(-1)?.kind !== "group");
  let copied = $state(false);

  async function copy() {
    await navigator.clipboard.writeText(msg.text);
    copied = true;
    setTimeout(() => (copied = false), 1200);
  }
</script>

{#if msg.role === "user"}
  <div class="user">
    {#if msg.attachments?.length}
      <div class="atts">
        {#each msg.attachments as path (path)}<AttachmentChip a={toAttachment(path)} />{/each}
      </div>
    {/if}
    <div class="bubble">{msg.text}</div>
  </div>
{:else}
  <div class="assistant">
    {#each blocks as b, i (b.kind === "group" ? b.id : b.part.id)}
      {#if b.kind === "group"}
        <ActivityGroup parts={b.parts} live={live && i === blocks.length - 1} />
      {:else}
        <PartView part={b.part} live={live && i === blocks.length - 1} />
      {/if}
    {/each}

    {#each approvals as a (a.key)}
      <ApprovalCard approval={a} />
    {/each}

    {#if busy}
      <div class="thinking">{notice ?? "Working…"}</div>
    {/if}

    {#if msg.error}
      <div class="error" class:soft={msg.error === "Stopped"}>{msg.error}</div>
    {/if}

    {#if !live && msg.text}
      <div class="tools">
        <button class="icon-btn" title="Copy" onclick={copy}>
          {#if copied}
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="m5 12 5 5 9-10" /></svg>
          {:else}
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="12" height="12" rx="2.5" /><path d="M5 15V5a2 2 0 0 1 2-2h10" /></svg>
          {/if}
        </button>
        {#if msg.model}<span class="model">{msg.model}</span>{/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .user { display: flex; flex-direction: column; align-items: flex-end; margin: 28px 0 18px; }
  .bubble {
    max-width: 78%; padding: 10px 16px; border-radius: 20px 20px 6px 20px;
    background: var(--bubble); white-space: pre-wrap; overflow-wrap: anywhere;
    font-size: 15px; line-height: 1.55; user-select: text; -webkit-user-select: text;
  }
  .assistant { margin-bottom: 8px; min-width: 0; max-width: 100%; }
  .thinking {
    display: inline-block; font-size: 13.5px; font-weight: 550; margin: 4px 0;
    background: linear-gradient(90deg, var(--faint) 0%, var(--text) 50%, var(--faint) 100%) 0 0 / 200% 100%;
    -webkit-background-clip: text; background-clip: text; color: transparent;
    animation: shimmer 1.6s linear infinite;
  }
  @keyframes shimmer { to { background-position: -200% 0; } }
  .error {
    margin-top: 8px; padding: 10px 14px; border-radius: 10px; font-size: 13.5px;
    color: var(--danger); background: var(--danger-bg); border: 1px solid color-mix(in srgb, var(--danger) 30%, transparent);
    user-select: text; -webkit-user-select: text;
  }
  .error.soft { color: var(--muted); background: none; border: 0; padding: 4px 0; font-size: 13px; }
  .tools { display: flex; align-items: center; gap: 6px; margin: 6px 0 0 -6px; opacity: 0; transition: opacity 0.12s; }
  .assistant:hover .tools { opacity: 1; }
  .model { font-size: 12px; color: var(--faint); }
  .atts { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; margin-bottom: 6px; }
</style>
