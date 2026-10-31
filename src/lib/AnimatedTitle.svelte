<script lang="ts">
  import { onDestroy, untrack } from "svelte";

  // When a chat is renamed (e.g. by the title model), the old title scrambles into the new one,
  // settling left to right in the accent color, then glows briefly. Switching to a different
  // chat (`id` changes) just swaps the text.
  let { text, id }: { text: string; id?: string } = $props();

  const GLYPHS = "abcdefghijklmnopqrstuvwxyz0123456789#%&*+<>/=";
  const DURATION = 650;

  let settled = $state(untrack(() => text));
  let scrambled = $state("");
  let glow = $state(false);
  let lastText = untrack(() => text);
  let lastId = untrack(() => id);
  let frame = 0;
  let glowTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const t = text;
    const k = id;
    const renamed = k === lastId && t !== lastText;
    lastText = t;
    lastId = k;
    // Untracked: the animation writes the state it reads, which must not re-run this effect.
    untrack(() => (renamed && !matchMedia("(prefers-reduced-motion: reduce)").matches ? decode(t) : stop(t)));
  });

  function stop(t: string) {
    cancelAnimationFrame(frame);
    clearTimeout(glowTimer);
    settled = t;
    scrambled = "";
    glow = false;
  }

  function decode(to: string) {
    stop(settled + scrambled);
    const start = performance.now();
    const from = settled;
    const tick = (now: number) => {
      const p = Math.min(1, (now - start) / DURATION);
      // Characters lock in left to right; the rest flicker, and the length eases toward the new one.
      const done = Math.floor(p * to.length);
      const len = Math.round(from.length + (to.length - from.length) * Math.min(1, p * 1.6));
      settled = to.slice(0, done);
      let rest = "";
      for (let i = done; i < Math.max(len, done); i++) {
        const c = to[i] ?? " ";
        rest += c === " " ? " " : GLYPHS[(Math.random() * GLYPHS.length) | 0];
      }
      scrambled = rest;
      if (p < 1) frame = requestAnimationFrame(tick);
      else {
        settled = to;
        scrambled = "";
        glow = true;
        glowTimer = setTimeout(() => (glow = false), 900);
      }
    };
    frame = requestAnimationFrame(tick);
  }

  onDestroy(() => {
    cancelAnimationFrame(frame);
    clearTimeout(glowTimer);
  });
</script>

<span class="at" class:glow>{settled}{#if scrambled}<span class="scramble">{scrambled}</span>{/if}</span>

<style>
  .at { border-radius: 4px; }
  .scramble { color: var(--accent); opacity: 0.75; font-family: var(--mono); font-size: 0.92em; }
  .glow { animation: glow 0.9s ease-out; }
  @keyframes glow {
    from { color: var(--accent); text-shadow: 0 0 12px color-mix(in srgb, var(--accent) 60%, transparent); }
    to { text-shadow: 0 0 12px transparent; }
  }
</style>
