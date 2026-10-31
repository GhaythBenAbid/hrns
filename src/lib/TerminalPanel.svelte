<script lang="ts">
  import { terms } from "./terminal.svelte";
  import TerminalView from "./TerminalView.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";

  // Bottom panel: tabs for every shell (including ones started from the phone), drag the top edge to resize.
  let { cwd }: { cwd: string | null | undefined } = $props();

  let views = $state<Record<string, TerminalView>>({});

  function resize(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const bottom = el.parentElement!.getBoundingClientRect().bottom;
    let raf = 0;
    const move = (ev: PointerEvent) => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => terms.setHeight(bottom - ev.clientY));
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("lostpointercapture", () => el.removeEventListener("pointermove", move), { once: true });
  }

  const home = (p: string) => p.replace(/^\/Users\/[^/]+/, "~");
</script>

<section class="panel" style:height="{terms.height}px" aria-label="Terminal">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grip" title="Drag to resize" onpointerdown={resize} ondblclick={() => terms.setHeight(260)}></div>
  <div class="bar">
    <div class="tabs" role="tablist">
      {#each terms.list as t (t.id)}
        <div class="tab" class:on={t.id === terms.active} class:dead={!t.alive} role="tab" tabindex="0" aria-selected={t.id === terms.active}
          title={home(t.cwd)}
          onclick={() => { terms.active = t.id; views[t.id]?.focus(); }}
          onkeydown={(e) => e.key === "Enter" && (terms.active = t.id)}
          onauxclick={(e) => e.button === 1 && terms.close(t.id)}>
          <ProjectIcon path={t.cwd} size={14} />
          <span class="name">{t.title}</span>
          <button class="x" title="Close terminal" aria-label="Close terminal {t.title}" onclick={(e) => { e.stopPropagation(); terms.close(t.id); }}>
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><path d="m7 7 10 10M17 7 7 17" /></svg>
          </button>
        </div>
      {/each}
      <button class="icon-btn add" title="New terminal in this project" aria-label="New terminal" onclick={() => terms.create(cwd)}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
      </button>
    </div>
    <button class="icon-btn" title="Hide terminal (⌘J)" aria-label="Hide terminal" onclick={() => terms.setOpen(false)}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6" /></svg>
    </button>
  </div>
  <div class="body">
    {#each terms.list as t (t.id)}
      <TerminalView bind:this={views[t.id]} id={t.id} visible={t.id === terms.active} />
    {/each}
    {#if !terms.list.length}
      <div class="empty">
        <button class="start" onclick={() => terms.create(cwd)}>Open a terminal{cwd ? ` in ${cwd.split("/").pop()}` : ""}</button>
      </div>
    {/if}
  </div>
</section>

<style>
  .panel { position: relative; flex: none; display: flex; flex-direction: column; border-top: 1px solid var(--border); background: var(--code-bg); min-height: 140px; }
  .grip { position: absolute; top: -4px; left: 0; right: 0; height: 8px; cursor: row-resize; z-index: 3; }
  .grip:hover, .grip:active { background: linear-gradient(transparent 3px, color-mix(in srgb, var(--accent) 60%, transparent) 3px 5px, transparent 5px); }
  .bar { flex: none; height: 34px; display: flex; align-items: center; gap: 6px; padding: 0 8px 0 10px; border-bottom: 1px solid var(--border); background: var(--sidebar); }
  .tabs { flex: 1; min-width: 0; display: flex; align-items: center; gap: 2px; overflow-x: auto; scrollbar-width: none; }
  .tab {
    flex: none; display: flex; align-items: center; gap: 6px; height: 26px; padding: 0 6px 0 9px; border-radius: 7px;
    font-size: 12px; color: var(--muted); cursor: default; user-select: none; -webkit-user-select: none;
    transition: background 0.12s, color 0.12s;
  }
  .tab:hover { background: var(--hover); color: var(--text); }
  .tab.on { background: var(--raised); color: var(--text); box-shadow: inset 0 0 0 1px var(--border); }
  .tab.dead .name { text-decoration: line-through; opacity: 0.6; }
  .name { max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 550; }
  .x { width: 18px; height: 18px; display: grid; place-items: center; border-radius: 5px; color: var(--faint); opacity: 0; transition: opacity 0.12s; }
  .tab:hover .x, .tab.on .x { opacity: 1; }
  .x:hover { background: var(--hover); color: var(--text); }
  .add, .bar > .icon-btn { width: 26px; height: 26px; flex: none; }
  .body { position: relative; flex: 1; min-height: 0; }
  .empty { position: absolute; inset: 0; display: grid; place-items: center; }
  .start { height: 32px; padding: 0 14px; border-radius: 8px; border: 1px solid var(--border); color: var(--muted); font-size: 13px; }
  .start:hover { color: var(--text); border-color: var(--faint); }
</style>
