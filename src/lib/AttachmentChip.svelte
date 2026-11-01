<script lang="ts">
  import { preview, type Attachment } from "./attachments";

  let { a, onremove }: { a: Attachment; onremove?: () => void } = $props();
  let src = $state<string | null>(null);
  $effect(() => {
    const path = a.path;
    src = null;
    if (a.image) preview(path).then((s) => { if (a.path === path) src = s; });
  });
</script>

<span class="chip" class:img={a.image && src} title={a.path}>
  {#if a.image && src}
    <img {src} alt={a.name} />
  {:else}
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8Z" /><path d="M14 3v5h5" /></svg>
    <span class="n">{a.name}</span>
  {/if}
  {#if onremove}
    <button class="x" title="Remove" onclick={onremove}>×</button>
  {/if}
</span>

<style>
  .chip {
    position: relative; display: inline-flex; align-items: center; gap: 6px; height: 30px; max-width: 220px; padding: 0 8px;
    border-radius: 8px; border: 1px solid var(--border); background: var(--raised); font-size: 12px; color: var(--muted);
  }
  .chip.img { height: 56px; width: 56px; padding: 0; overflow: hidden; }
  img { width: 100%; height: 100%; object-fit: cover; }
  .n { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .x {
    width: 18px; height: 18px; border-radius: 50%; display: grid; place-items: center; font-size: 13px; line-height: 1; color: var(--muted);
  }
  .chip.img .x { position: absolute; top: 3px; right: 3px; background: rgb(0 0 0 / 0.6); color: #fff; }
  .x:hover { color: var(--text); background: var(--hover); }
</style>
