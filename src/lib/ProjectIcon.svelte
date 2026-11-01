<script lang="ts" module>
  import { invoke } from "@tauri-apps/api/core";
  import * as si from "simple-icons";

  type Icon = { image: string | null; tech: string | null };
  type Logo = { title: string; hex: string; path: string };

  /** Backend tech ids → Simple Icons logos. */
  const LOGOS: Record<string, Logo> = {
    tauri: si.siTauri, electron: si.siElectron, expo: si.siExpo, next: si.siNextdotjs, nuxt: si.siNuxt,
    astro: si.siAstro, remix: si.siRemix, svelte: si.siSvelte, angular: si.siAngular, solid: si.siSolid,
    qwik: si.siQwik, vue: si.siVuedotjs, react: si.siReact, rust: si.siRust, go: si.siGo, flutter: si.siFlutter,
    dart: si.siDart, swift: si.siSwift, rails: si.siRubyonrails, ruby: si.siRuby, laravel: si.siLaravel,
    php: si.siPhp, elixir: si.siElixir, deno: si.siDeno, godot: si.siGodotengine, django: si.siDjango,
    python: si.siPython, kotlin: si.siKotlin, java: si.siOpenjdk, typescript: si.siTypescript,
    javascript: si.siJavascript, c: si.siC, cplusplus: si.siCplusplus, dotnet: si.siDotnet, lua: si.siLua,
    zig: si.siZig, haskell: si.siHaskell, scala: si.siScala, clojure: si.siClojure, ocaml: si.siOcaml,
    julia: si.siJulia, r: si.siR, perl: si.siPerl, bash: si.siGnubash, gleam: si.siGleam, html: si.siHtml5,
    css: si.siCss,
  };

  // One lookup per folder per session; the backend reads a few files each time.
  const cache = new Map<string, Promise<Icon>>();
  function load(path: string) {
    let p = cache.get(path);
    if (!p) {
      p = invoke<Icon>("project_icon", { path }).catch(() => ({ image: null, tech: null }));
      cache.set(path, p);
    }
    return p;
  }

  /** Logos in pure black or white would vanish on one theme; draw those in the text color. */
  function ink(hex: string) {
    const [r, g, b] = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    return l < 0.12 || l > 0.9 ? null : `#${hex}`;
  }

  // Fallback like T3 Code: two-letter monogram, color picked from the name.
  const HUES = [262, 217, 174, 142, 38, 24, 350, 322, 199, 84];
  function monogram(name: string) {
    const words = name.match(/[\p{L}\p{N}]+/gu) ?? [];
    const first = [...(words[0] ?? "?")];
    const second = words.length > 1 ? [...words[words.length - 1]][0] : (first[1] ?? "");
    let h = 0;
    for (const ch of name.toLowerCase()) h = (h * 31 + ch.codePointAt(0)!) % HUES.length;
    return { text: (first[0] + second).toUpperCase(), hue: HUES[h] };
  }
</script>

<script lang="ts">
  let { path, size = 16 }: { path: string | null | undefined; size?: number } = $props();

  let el = $state<HTMLElement>();
  let icon = $state<Icon | null>(null);
  let broken = $state(false);

  const name = $derived(path?.split("/").filter(Boolean).pop() ?? "");
  const logo = $derived(icon?.tech ? LOGOS[icon.tech] : undefined);
  const mono = $derived(monogram(name));

  // Resolve only once on screen: the finder can list hundreds of folders.
  $effect(() => {
    const p = path;
    icon = null;
    broken = false;
    if (!p || !el) return;
    let live = true;
    const io = new IntersectionObserver(([e]) => {
      if (!e.isIntersecting) return;
      io.disconnect();
      load(p).then((i) => live && (icon = i));
    });
    io.observe(el);
    return () => {
      live = false;
      io.disconnect();
    };
  });
</script>

<span
  bind:this={el}
  class="pi"
  style:--s="{size}px"
  title={logo ? `${name} · ${logo.title}` : name}
  aria-hidden="true"
>
  {#if icon?.image && !broken}
    <img src={icon.image} alt="" onerror={() => (broken = true)} />
  {:else if logo}
    <span class="tile" style:--c={ink(logo.hex) ?? "var(--text)"}>
      <svg viewBox="0 0 24 24"><path d={logo.path} /></svg>
    </span>
  {:else if icon && name}
    <span class="tile mono" style:--h={mono.hue}>{mono.text}</span>
  {:else}
    <span class="tile blank"></span>
  {/if}
</span>

<style>
  .pi { flex: none; width: var(--s); height: var(--s); display: inline-grid; place-items: center; }
  img { width: 100%; height: 100%; object-fit: contain; border-radius: calc(var(--s) * 0.22); }
  .tile {
    width: 100%; height: 100%; display: grid; place-items: center; border-radius: calc(var(--s) * 0.28);
    background: color-mix(in srgb, var(--c, var(--muted)) 16%, var(--raised));
    animation: in 0.18s ease-out;
  }
  .tile svg { width: 66%; height: 66%; fill: var(--c); }
  .mono {
    font-size: calc(var(--s) * 0.42); font-weight: 700; letter-spacing: -0.02em; line-height: 1;
    color: hsl(var(--h) 70% 68%); background: hsl(var(--h) 45% 50% / 0.18);
  }
  :global([data-theme="light"]) .mono { color: hsl(var(--h) 60% 38%); }
  .blank { background: var(--raised); animation: none; }
  @keyframes in { from { opacity: 0; transform: scale(0.85); } }
</style>
