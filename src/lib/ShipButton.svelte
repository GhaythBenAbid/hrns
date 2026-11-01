<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app, type Pane } from "./store.svelte";
  import { refreshDiff } from "./gitdiff.svelte";

  type Status = {
    branch: string | null; base: string | null; dirty: number; ahead: number; behind: number;
    unpushed: number | null; hasRemote: boolean; isWorktree: boolean;
  };

  let { pane }: { pane: Pane } = $props();
  const id = $derived(pane.current?.id ?? null);

  let open = $state(false);
  let s = $state<Status | null>(null);
  let loaded = $state(false);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let done = $state<{ text: string; url?: string } | null>(null);
  let msg = $state("");

  async function load() {
    if (!id) return;
    try {
      s = await invoke<Status | null>("ship_status", { id });
    } catch {
      s = null;
    }
    loaded = true;
  }

  // Know whether this chat is in a git repo (to show the button at all); refresh when its turn ends.
  $effect(() => {
    id;
    pane.current?.messages.length;
    s = null;
    loaded = false;
    load();
  });

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    open = !open;
    if (open) {
      error = null;
      done = null;
      msg = pane.current?.title ?? "";
      load();
    }
  }

  async function act(name: string, f: () => Promise<{ text: string; url?: string } | void>) {
    busy = name;
    error = null;
    done = null;
    try {
      done = (await f()) ?? null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
      await load();
      refreshDiff(pane.current?.cwd);
      if (pane.current?.worktree) refreshDiff(pane.current.worktree.root);
    }
  }

  const commit = () =>
    act("commit", async () => ({ text: `Committed ${await invoke<string>("ship_commit", { id, message: msg })}` }));
  const push = () => act("push", async () => { await invoke("ship_push", { id }); return { text: "Pushed" }; });
  const pr = () =>
    act("pr", async () => {
      const body = `Made with hrns from the chat “${pane.current?.title ?? ""}”.`;
      const r = await invoke<{ url: string; compare: boolean }>("ship_pr", { id, title: msg || pane.current?.title, body });
      await openUrl(r.url);
      return r.compare
        ? { text: "Pushed. Opened GitHub to create the pull request (install the GitHub CLI to create it from here).", url: r.url }
        : { text: "Pull request is open", url: r.url };
    });
  const merge = () =>
    act("merge", async () => ({ text: `Merged into ${s?.base} (${await invoke<string>("ship_merge", { id })}). You can delete this chat's worktree from the sidebar now.` }));

  const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;
  const isBase = $derived(!!s && s.branch === s.base);
</script>

<svelte:window onclick={() => (open = false)} onkeydown={(e) => e.key === "Escape" && (open = false)} />

{#if loaded && s}
  <div class="ship">
    <button class="ship-btn" class:on={open} title="Commit, push, open a PR{s.isWorktree ? ' or merge' : ''}" onclick={toggle}>
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3.5" /><path d="M3 12h5.5M15.5 12H21" /></svg>
      <span>Ship</span>
      {#if s.dirty}<span class="dot" title="{plural(s.dirty, 'uncommitted file')}"></span>{/if}
    </button>

    {#if open}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="pop" onclick={(e) => e.stopPropagation()}>
        <div class="branchline">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><circle cx="6" cy="5" r="2" /><circle cx="6" cy="19" r="2" /><circle cx="18" cy="7" r="2" /><path d="M6 7v10M18 9c0 4-6 3-11.5 8" /></svg>
          <code>{s.branch ?? "detached HEAD"}</code>
          {#if s.base && !isBase}<span class="arrow">→</span><code>{s.base}</code>{/if}
        </div>
        <div class="facts">
          <span class:hot={s.dirty > 0}>{s.dirty ? plural(s.dirty, "uncommitted file") : "No uncommitted changes"}</span>
          {#if s.base && !isBase}
            <span>{plural(s.ahead, "commit")} ahead{s.behind ? `, ${s.behind} behind` : ""}</span>
          {/if}
          {#if s.hasRemote}
            <span>{s.unpushed == null ? "Not pushed yet" : s.unpushed ? `${s.unpushed} to push` : "Pushed"}</span>
          {/if}
        </div>

        <section>
          <div class="label">Commit all changes</div>
          <div class="commit">
            <input bind:value={msg} placeholder="Commit message" spellcheck="true" onkeydown={(e) => e.key === "Enter" && s?.dirty && commit()} />
            <button class="primary" disabled={!s.dirty || !msg.trim() || !!busy} onclick={commit}>{busy === "commit" ? "Committing…" : "Commit"}</button>
          </div>
        </section>

        {#if s.hasRemote}
          <section class="row">
            <button disabled={!!busy || (s.unpushed === 0) || (!s.ahead && s.unpushed == null)} onclick={push}>{busy === "push" ? "Pushing…" : "Push"}</button>
            <button disabled={!!busy || isBase || !s.branch} title={isBase ? `You're on ${s.base}` : `Push and open a pull request into ${s.base}`} onclick={pr}>{busy === "pr" ? "Opening…" : "Open pull request"}</button>
          </section>
        {/if}

        {#if s.isWorktree}
          <section>
            <button class="merge" disabled={!!busy || !s.ahead || s.dirty > 0} title={s.dirty ? "Commit the changes first" : !s.ahead ? "No commits to merge" : ""} onclick={merge}>
              {busy === "merge" ? "Merging…" : `Merge into ${s.base}`}
            </button>
            <div class="hint">Merges this worktree's branch into {s.base} in your project folder. Stops without changing anything if they conflict.</div>
          </section>
        {/if}

        {#if error}<div class="err">{error}</div>{/if}
        {#if done}
          <div class="ok">
            {done.text}
            {#if done.url}<button class="link" onclick={() => openUrl(done!.url!)}>{done.url.replace(/^https:\/\//, "")}</button>{/if}
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .ship { position: relative; flex: none; }
  .ship-btn {
    height: 26px; padding: 0 9px; border-radius: 8px; display: inline-flex; align-items: center; gap: 6px;
    border: 1px solid var(--border); background: var(--panel); color: var(--muted); font-size: 12px; font-weight: 550;
  }
  .ship-btn:hover, .ship-btn.on { background: var(--hover); color: var(--text); border-color: color-mix(in srgb, var(--accent) 35%, var(--border)); }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: #e0af68; }
  .pop {
    position: absolute; z-index: 40; top: calc(100% + 8px); right: 0; width: 340px; max-width: calc(100vw - 24px);
    padding: 12px; border-radius: 14px; background: var(--panel); border: 1px solid var(--border);
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.35); display: flex; flex-direction: column; gap: 10px; font-size: 13px;
  }
  .branchline { display: flex; align-items: center; gap: 6px; min-width: 0; color: var(--muted); }
  .branchline code { font: 500 12px var(--mono); color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arrow { color: var(--faint); }
  .facts { display: flex; flex-wrap: wrap; gap: 4px 12px; font-size: 12px; color: var(--muted); }
  .facts .hot { color: #e0af68; }
  section { display: flex; flex-direction: column; gap: 6px; padding-top: 10px; border-top: 1px solid var(--border); }
  section.row { flex-direction: row; }
  section.row button { flex: 1; }
  .label { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--faint); }
  .commit { display: flex; gap: 6px; }
  input {
    flex: 1; min-width: 0; height: 32px; padding: 0 10px; border-radius: 8px; border: 1px solid var(--border);
    background: var(--bg); color: var(--text); font-size: 13px; outline: none; user-select: text; -webkit-user-select: text;
  }
  input:focus { border-color: color-mix(in srgb, var(--accent) 50%, var(--border)); }
  section button, .commit button {
    height: 32px; padding: 0 12px; border-radius: 8px; border: 1px solid var(--border); background: var(--raised);
    color: var(--text); font-size: 12.5px; font-weight: 550; white-space: nowrap;
  }
  section button:hover:not(:disabled) { background: var(--hover); }
  .primary, .merge { background: var(--text) !important; color: var(--bg) !important; border-color: transparent !important; }
  button:disabled { opacity: 0.4; cursor: default; }
  .hint { font-size: 11.5px; color: var(--faint); line-height: 1.4; }
  .err { font-size: 12px; color: var(--danger); line-height: 1.4; user-select: text; -webkit-user-select: text; }
  .ok { font-size: 12px; color: #4ade80; line-height: 1.4; display: flex; flex-direction: column; gap: 2px; }
  .link { text-align: left; color: var(--accent); font-size: 12px; text-decoration: underline; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
