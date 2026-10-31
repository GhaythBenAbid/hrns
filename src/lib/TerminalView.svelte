<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { onMount } from "svelte";
  import { ptySize, terms } from "./terminal.svelte";

  // One xterm per session. Views stay mounted while hidden so switching tabs keeps the screen.
  let { id, visible }: { id: string; visible: boolean } = $props();

  let host = $state<HTMLDivElement>();
  let term: Terminal | null = null;
  let fit: FitAddon | null = null;
  let lastSize = "";

  const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

  function theme() {
    const light = document.documentElement.dataset.theme === "light";
    const bg = css("--code-bg") || (light ? "#f7f7f8" : "#0b0b0d");
    const accent = css("--accent");
    const ansi = light
      ? ["#3f3f46", "#dc2626", "#16a34a", "#b45309", "#2563eb", "#9333ea", "#0891b2", "#71717a", "#52525b", "#ef4444", "#22c55e", "#d97706", "#3b82f6", "#a855f7", "#06b6d4", "#18181b"]
      : ["#27272a", "#f87171", "#4ade80", "#fbbf24", "#60a5fa", "#c084fc", "#22d3ee", "#d4d4d8", "#52525b", "#fca5a5", "#86efac", "#fde68a", "#93c5fd", "#d8b4fe", "#67e8f9", "#fafafa"];
    const [black, red, green, yellow, blue, magenta, cyan, white, brightBlack, brightRed, brightGreen, brightYellow, brightBlue, brightMagenta, brightCyan, brightWhite] = ansi;
    return {
      background: bg, foreground: css("--text"), cursor: accent, cursorAccent: bg,
      selectionBackground: /^#[0-9a-f]{6}$/i.test(accent) ? accent + "4d" : "#7c7c8a4d",
      black, red, green, yellow, blue, magenta, cyan, white,
      brightBlack, brightRed, brightGreen, brightYellow, brightBlue, brightMagenta, brightCyan, brightWhite,
    };
  }

  function resize() {
    if (!term || !fit || !visible || !host?.offsetWidth) return;
    try {
      fit.fit();
    } catch {
      return;
    }
    claim();
  }

  /** Make the shell match this view (a phone may have resized it since). */
  function claim() {
    if (!term) return;
    const size = `${term.cols}x${term.rows}`;
    if (size !== lastSize || ptySize.get(id) !== size) {
      lastSize = size;
      ptySize.set(id, size);
      invoke("term_resize", { id, cols: term.cols, rows: term.rows }).catch(() => {});
    }
  }

  export function focus() {
    term?.focus();
  }

  onMount(() => {
    const t = new Terminal({
      fontFamily: css("--mono") || "ui-monospace, Menlo, monospace",
      fontSize: 12.5,
      lineHeight: 1.2,
      cursorBlink: true,
      scrollback: 10000,
      theme: theme(),
      allowProposedApi: true,
      macOptionIsMeta: true,
    });
    const f = new FitAddon();
    t.loadAddon(f);
    t.open(host!);
    term = t;
    fit = f;

    // Earlier output first (the session may have started on the phone), then live output.
    let ready = false;
    const queued: string[] = [];
    const detach = terms.attach(
      id,
      (d) => (ready ? t.write(d) : queued.push(d)),
      () => t.write("\r\n\x1b[2m[process exited]\x1b[0m\r\n"),
    );
    invoke<string>("term_scrollback", { id })
      .then((sb) => t.write(sb))
      .catch(() => {})
      .finally(() => {
        ready = true;
        queued.forEach((d) => t.write(d));
      });

    const input = t.onData((data) => {
      claim();
      invoke("term_write", { id, data }).catch(() => {});
    });

    // Follow theme, accent and code font changes.
    const mo = new MutationObserver(() => {
      t.options.theme = theme();
      t.options.fontFamily = css("--mono") || t.options.fontFamily;
      resize();
    });
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "style"] });

    let raf = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(resize);
    });
    ro.observe(host!);

    return () => {
      detach();
      input.dispose();
      mo.disconnect();
      ro.disconnect();
      cancelAnimationFrame(raf);
      t.dispose();
      term = null;
    };
  });

  $effect(() => {
    if (visible) requestAnimationFrame(() => { resize(); term?.focus(); });
  });
</script>

<div class="view" class:hidden={!visible}>
  <div class="host" bind:this={host}></div>
</div>

<style>
  .view { position: absolute; inset: 0; padding: 6px 4px 4px 12px; background: var(--code-bg); }
  .view.hidden { visibility: hidden; pointer-events: none; }
  .host { width: 100%; height: 100%; }
  .host :global(.xterm-viewport) { scrollbar-width: thin; background: transparent !important; }
</style>
