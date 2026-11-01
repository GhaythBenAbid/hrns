<script lang="ts">
  import { fade } from "svelte/transition";

  const word = "hrns".split("");
</script>

<!-- Shown while Codex starts: the prompt mark draws itself, the cursor blinks, the wordmark rises in. -->
<div class="splash" out:fade={{ duration: 220 }} role="status" aria-label="Starting Codex">
  <div class="stage">
    <div class="glow"></div>
    <div class="ring"></div>
    <div class="tile">
      <svg width="34" height="34" viewBox="0 0 24 24" fill="none" stroke-linecap="round" stroke-linejoin="round">
        <path class="chev" d="M6 7l5 5-5 5" stroke="currentColor" stroke-width="2.4" pathLength="1" />
        <path class="cursor" d="M13 17h6" stroke="var(--accent)" stroke-width="2.4" />
      </svg>
    </div>
  </div>

  <div class="word" aria-hidden="true">
    {#each word as ch, i}
      <span style:animation-delay="{0.55 + i * 0.07}s">{ch}</span>
    {/each}
  </div>

  <div class="status">
    <div class="track"><div class="bar"></div></div>
    <span>Starting Codex</span>
  </div>
</div>

<style>
  /* Animatable so the ring's highlight orbits while the rounded square stays put. */
  @property --angle { syntax: "<angle>"; inherits: false; initial-value: 0deg; }

  .splash { flex: 1; display: grid; place-content: center; justify-items: center; gap: 22px; padding: 24px; }

  .stage { position: relative; width: 120px; height: 120px; display: grid; place-items: center; }
  .glow {
    position: absolute; inset: 6px; border-radius: 50%;
    background: radial-gradient(circle, color-mix(in srgb, var(--accent) 38%, transparent), transparent 68%);
    filter: blur(14px); animation: breathe 2.8s ease-in-out infinite;
  }
  .ring {
    position: absolute; inset: 14px; border-radius: 30px; padding: 1.5px;
    background: conic-gradient(from var(--angle), transparent 0 62%, color-mix(in srgb, var(--accent) 85%, transparent) 82%, transparent 100%);
    -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0);
    mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0);
    -webkit-mask-composite: xor; mask-composite: exclude;
    opacity: 0; animation: appear 0.6s 0.35s ease-out forwards, spin 2.4s linear infinite;
  }
  .tile {
    position: relative; width: 76px; height: 76px; border-radius: 24px; display: grid; place-items: center;
    background: linear-gradient(160deg, var(--raised), var(--panel)); border: 1px solid var(--border);
    box-shadow: 0 14px 40px rgb(0 0 0 / 0.35);
    animation: pop 0.6s cubic-bezier(0.32, 0.72, 0, 1) both;
  }
  .chev { stroke-dasharray: 1; stroke-dashoffset: 1; animation: draw 0.5s 0.25s cubic-bezier(0.65, 0, 0.35, 1) forwards; }
  .cursor { opacity: 0; animation: blink 1.05s 0.7s steps(1, end) infinite; }

  .word { display: flex; font-size: 22px; font-weight: 650; letter-spacing: -0.02em; }
  .word span { opacity: 0; transform: translateY(8px); animation: rise 0.5s cubic-bezier(0.32, 0.72, 0, 1) forwards; }

  .status { display: grid; justify-items: center; gap: 10px; opacity: 0; animation: appear 0.5s 0.9s ease-out forwards; }
  .status span { font-size: 12px; color: var(--faint); letter-spacing: 0.02em; }
  .track { position: relative; width: 120px; height: 2px; border-radius: 2px; overflow: hidden; background: var(--raised); }
  .bar {
    position: absolute; inset: 0; width: 40%; border-radius: 2px;
    background: linear-gradient(90deg, transparent, var(--accent), transparent);
    animation: sweep 1.3s cubic-bezier(0.45, 0, 0.55, 1) infinite;
  }

  @keyframes pop { from { opacity: 0; transform: scale(0.82); } }
  @keyframes draw { to { stroke-dashoffset: 0; } }
  @keyframes blink { 0% { opacity: 1; } 50% { opacity: 0; } }
  @keyframes rise { to { opacity: 1; transform: none; } }
  @keyframes appear { to { opacity: 1; } }
  @keyframes spin { to { --angle: 360deg; } }
  @keyframes breathe { 0%, 100% { opacity: 0.45; transform: scale(0.92); } 50% { opacity: 0.9; transform: scale(1.05); } }
  @keyframes sweep { from { transform: translateX(-100%); } to { transform: translateX(250%); } }

  @media (prefers-reduced-motion: reduce) {
    .splash *, .word span { animation: none !important; opacity: 1 !important; transform: none !important; stroke-dashoffset: 0 !important; }
    .ring { opacity: 0 !important; }
  }
</style>
