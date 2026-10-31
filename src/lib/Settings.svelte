<script lang="ts">
  import { untrack } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import QRCode from "qrcode";
  import { app, type Access } from "./store.svelte";

  const sections = [
    { id: "general", label: "General", icon: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" },
    { id: "models", label: "Models", icon: "M12 2 2 7l10 5 10-5ZM2 17l10 5 10-5M2 12l10 5 10-5" },
    { id: "permissions", label: "Permissions", icon: "M12 3 4 6v6c0 4.5 3.4 8.2 8 9 4.6-.8 8-4.5 8-9V6Z" },
    { id: "remote", label: "Remote", icon: "M7 2h10a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2ZM11 18h2" },
    { id: "account", label: "Account", icon: "M20 21a8 8 0 0 0-16 0M12 13a5 5 0 1 0 0-10 5 5 0 0 0 0 10Z" },
    { id: "about", label: "About", icon: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM12 16v-4M12 8h.01" },
  ] as const;
  type Section = (typeof sections)[number]["id"];

  const section = $derived(app.settingsSection as Section);
  let version = $state("");
  getVersion().then((v) => (version = v)).catch(() => {});

  type RemoteInfo = { enabled: boolean; running: boolean; url: string | null; token: string; pairUrl: string | null };
  let remote = $state<RemoteInfo | null>(null);
  let remoteBusy = $state(false);
  let remoteError = $state<string | null>(null);
  let qr = $state("");
  let showToken = $state(false);
  let testResult = $state<string | null>(null);
  async function testNotify() {
    const phones = await invoke<number>("remote_test_notify");
    testResult = phones ? `Sent to ${phones} phone${phones > 1 ? "s" : ""}.` : "No phone is connected right now.";
    setTimeout(() => (testResult = null), 4000);
  }

  async function remoteCall(cmd: string) {
    remoteBusy = true;
    remoteError = null;
    try {
      remote = await invoke<RemoteInfo>(cmd);
    } catch (e) {
      remoteError = String(e);
    } finally {
      remoteBusy = false;
    }
  }
  $effect(() => {
    if (section === "remote") untrack(() => remoteCall("remote_info"));
  });
  $effect(() => {
    const url = remote?.pairUrl;
    if (!url) return void (qr = "");
    QRCode.toString(url, { type: "svg", margin: 1, color: { dark: "#0b0b0d", light: "#ffffff" } }).then((s) => (qr = s));
  });

  // Refresh usage when the Accounts page opens (untracked: refreshing updates app.accounts).
  $effect(() => {
    if (section === "account") untrack(() => app.accounts.filter((a) => a.email).forEach((a) => app.refreshStatus(a.id)));
  });

  const accessModes: { id: Access; label: string; desc: string }[] = [
    { id: "read-only", label: "Read only", desc: "Codex can read files. Asks before editing or running commands." },
    { id: "auto", label: "Auto", desc: "Edits files and runs commands inside the project. Asks for anything outside it or for network." },
    { id: "full", label: "Full access", desc: "Can edit anything and use the network without asking. Use with care." },
  ];
  const sizes = [
    { id: "small", label: "Small" },
    { id: "default", label: "Default" },
    { id: "large", label: "Large" },
  ] as const;

  const efforts = $derived(app.currentModel?.supportedReasoningEfforts ?? []);
  const effort = $derived(app.effort ?? app.currentModel?.defaultReasoningEffort ?? "");

  function pct(w: { usedPercent: number } | null | undefined) {
    return Math.max(0, Math.min(100, Math.round(w?.usedPercent ?? 0)));
  }
  function resets(ts: number | undefined) {
    if (!ts) return "";
    const d = new Date(ts * 1000);
    return d.toLocaleString(undefined, { weekday: "short", hour: "numeric", minute: "2-digit" });
  }
</script>

<div class="settings">
  <nav>
    <button class="back" onclick={() => (app.view = "chat")}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M15 18l-6-6 6-6" /></svg>
      Back to chat
    </button>
    {#each sections as s (s.id)}
      <button class="nav" class:sel={section === s.id} onclick={() => (app.settingsSection = s.id)}>
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d={s.icon} /></svg>
        {s.label}
      </button>
    {/each}
  </nav>

  <div class="body">
    <div class="inner">
      {#if section === "general"}
        <h1>General</h1>

        <div class="group">
          <div class="row">
            <div class="text">
              <div class="label">Default project folder</div>
              <div class="hint">New chats start here unless you pick another folder.</div>
            </div>
            <button class="btn path" title={app.folder ?? ""} onclick={() => app.pickFolder()}>
              {app.folder ? app.folder.replace(/^\/Users\/[^/]+/, "~") : "Choose…"}
            </button>
          </div>
          <div class="row">
            <div class="text">
              <div class="label">Text size</div>
              <div class="hint">Size of messages and interface text.</div>
            </div>
            <div class="seg">
              {#each sizes as z (z.id)}
                <button class:sel={app.textSize === z.id} onclick={() => app.setTextSize(z.id)}>{z.label}</button>
              {/each}
            </div>
          </div>
        </div>

        <h2>Keyboard shortcuts</h2>
        <div class="group keys">
          {#each [["New chat", "⌘N"], ["Search chats", "⌘K"], ["Toggle sidebar", "⌘B"], ["Settings", "⌘,"], ["Close Settings / Usage", "Esc"], ["Send message", "↵"], ["New line", "⇧↵"], ["Approve request", "⌘↵"], ["Stop response", "Esc"]] as [label, key]}
            <div class="row compact"><span>{label}</span><kbd>{key}</kbd></div>
          {/each}
        </div>

      {:else if section === "models"}
        <h1>Models</h1>
        <p class="lead">The default model and reasoning level for new messages. You can still switch per message from the composer.</p>

        <div class="group">
          {#each app.models as m (m.id)}
            <button class="row option" class:sel={m.id === app.model} onclick={() => app.setModel(m.id)}>
              <span class="radio"></span>
              <div class="text">
                <div class="label">{m.displayName}{#if m.isDefault}<span class="tag">Codex default</span>{/if}</div>
                <div class="hint">{m.description}</div>
              </div>
            </button>
          {:else}
            <div class="row"><span class="hint">Sign in to load models.</span></div>
          {/each}
        </div>

        {#if efforts.length}
          <h2>Reasoning effort</h2>
          <div class="group">
            {#each efforts as e (e.reasoningEffort)}
              <button class="row option" class:sel={e.reasoningEffort === effort} onclick={() => app.setEffort(e.reasoningEffort)}>
                <span class="radio"></span>
                <div class="text">
                  <div class="label cap">{e.reasoningEffort}</div>
                  <div class="hint">{e.description}</div>
                </div>
              </button>
            {/each}
          </div>
        {/if}

      {:else if section === "permissions"}
        <h1>Permissions</h1>
        <p class="lead">What Codex may do in your project without asking. Applies to new messages; you can change it per chat from the composer.</p>
        <div class="group">
          {#each accessModes as m (m.id)}
            <button class="row option" class:sel={m.id === app.access} onclick={() => app.setAccess(m.id)}>
              <span class="radio" class:danger={m.id === "full"}></span>
              <div class="text">
                <div class="label">{m.label}</div>
                <div class="hint">{m.desc}</div>
              </div>
            </button>
          {/each}
        </div>

      {:else if section === "remote"}
        <h1>Remote</h1>
        <p class="lead">Control your chats from your phone over Tailscale: see every chat, follow it live, send messages and approve commands. Only devices on your tailnet can reach it, and each request needs the pairing token.</p>

        <div class="group">
          <div class="row">
            <div class="text">
              <div class="label">Remote access {#if remote?.running}<span class="tag live">On</span>{/if}</div>
              <div class="hint">{remote?.running ? `Listening on ${remote.url} (Tailscale only)` : "Off — nothing is listening."}</div>
            </div>
            {#if remote?.running}
              <button class="btn" disabled={remoteBusy} onclick={() => remoteCall("remote_disable")}>Turn off</button>
            {:else}
              <button class="btn primary" disabled={remoteBusy} onclick={() => remoteCall("remote_enable")}>{remoteBusy ? "Starting…" : "Turn on"}</button>
            {/if}
          </div>
        </div>
        {#if remoteError}<div class="remote-err">{remoteError}</div>{/if}

        {#if remote?.running && qr}
          <div class="pair">
            <div class="qr">{@html qr}</div>
            <div class="pair-text">
              <div class="label">Pair your phone</div>
              <ol>
                <li>Install <b>Tailscale</b> on your phone and sign in to the same tailnet.</li>
                <li>Open the <b>hrns</b> mobile app and tap <b>Scan pairing code</b>.</li>
                <li>Point it at this QR code.</li>
              </ol>
              <div class="hint mono">{remote.url}</div>
              <div class="token-row">
                <code class="token">{showToken ? remote.token : "•".repeat(24)}</code>
                <button class="btn small" onclick={() => (showToken = !showToken)}>{showToken ? "Hide" : "Show"} token</button>
                <button class="btn small danger" onclick={() => remoteCall("remote_rotate")} title="Unpairs every phone">Reset</button>
              </div>
              <div class="token-row">
                <button class="btn small" onclick={testNotify}>Send test notification</button>
                {#if testResult}<span class="hint">{testResult}</span>{/if}
              </div>
            </div>
          </div>
        {/if}

      {:else if section === "account"}
        <h1>Accounts</h1>
        <p class="lead">Connect several ChatGPT accounts. New chats use the selected account; each chat stays on the account it started with.</p>

        <div class="accounts">
          {#each app.accounts as a (a.id)}
            {@const st = app.statuses[a.id]}
            {@const active = a.id === app.activeAccount}
            <div class="acard" class:active>
              <div class="row">
                <div class="who">
                  <div class="avatar" class:off={!a.email}>{(a.email ?? "?")[0].toUpperCase()}</div>
                  <div class="text">
                    <div class="label">
                      {a.email ?? (app.pendingLogin === a.id ? "Waiting for sign-in in your browser…" : "Not signed in")}
                      {#if active}<span class="tag">Selected</span>{/if}
                      {#if a.id === "default"}<span class="tag muted">Codex CLI</span>{/if}
                    </div>
                    <div class="hint cap">{a.plan ? `ChatGPT ${a.plan}` : a.id === "default" ? "Uses your ~/.codex login" : "Separate login"}</div>
                  </div>
                </div>
                <div class="btns">
                  {#if a.email}
                    {#if !active}<button class="btn" onclick={() => app.setActiveAccount(a.id)}>Use for new chats</button>{/if}
                    <button class="btn danger" onclick={() => app.logout(a.id)}>Sign out</button>
                  {:else}
                    <button class="btn primary" onclick={() => app.login(a.id)}>Sign in</button>
                  {/if}
                  {#if a.id !== "default"}
                    <button class="btn icon danger" title="Remove account" onclick={() => app.removeAccount(a.id)}>
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3" /></svg>
                    </button>
                  {/if}
                </div>
              </div>
              {#if st?.rateLimits}
                <div class="usage-grid">
                  {#each [["primary", "5-hour"], ["secondary", "Weekly"]] as [key, label]}
                    {@const w = st.rateLimits[key as "primary" | "secondary"]}
                    {#if w}
                      <div class="u">
                        <div class="u-top"><span>{label}</span><span>{pct(w)}%</span></div>
                        <div class="bar"><div style="width:{pct(w)}%" class:hot={pct(w) > 80}></div></div>
                        <div class="hint">Resets {resets(w.resetsAt)}</div>
                      </div>
                    {/if}
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>

        <button class="add" onclick={() => app.addAccount()} disabled={!!app.pendingLogin}>
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
          {app.pendingLogin ? "Finish signing in in your browser…" : "Add ChatGPT account"}
        </button>

      {:else if section === "about"}
        <h1>About</h1>
        <div class="group">
          <div class="row compact"><span>hrns</span><span class="hint">v{version}</span></div>
          <div class="row compact"><span>Engine</span><span class="hint">Codex app-server (your ChatGPT subscription)</span></div>
          <div class="row compact"><span>Chat history</span><span class="hint">Stored locally on this Mac</span></div>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .settings { flex: 1; min-height: 0; display: flex; }
  nav { width: 288px; flex: none; padding: 4px 16px 16px; border-right: 1px solid var(--border); display: flex; flex-direction: column; gap: 2px; }
  .back { display: flex; align-items: center; gap: 6px; padding: 6px 8px; margin-bottom: 12px; font-size: 13px; color: var(--muted); border-radius: 8px; }
  .back:hover { color: var(--text); background: var(--hover); }
  .nav { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 9px; font-size: 13.5px; color: var(--muted); }
  .nav:hover { background: var(--hover); color: var(--text); }
  .nav.sel { background: var(--raised); color: var(--text); font-weight: 550; }

  .body { flex: 1; min-width: 0; overflow-y: auto; }
  .inner { max-width: 920px; margin: 0 auto; padding: 8px 48px 64px; }
  h1 { font-size: 22px; font-weight: 650; letter-spacing: -0.01em; margin: 0 0 18px; }
  h2 { font-size: 13px; font-weight: 600; color: var(--muted); margin: 28px 0 10px; }
  .lead { color: var(--muted); font-size: 13.5px; margin: -8px 0 18px; line-height: 1.5; }

  .group { border: 1px solid var(--border); border-radius: 14px; background: var(--panel); overflow: hidden; }
  .row {
    width: 100%; display: flex; align-items: center; justify-content: space-between; gap: 16px;
    padding: 14px 16px; text-align: left;
  }
  .row + .row { border-top: 1px solid var(--border); }
  .row.compact { padding: 10px 16px; font-size: 13.5px; }
  .text { min-width: 0; flex: 1; }
  .label { font-size: 13.5px; font-weight: 550; display: flex; align-items: center; gap: 8px; }
  .hint { font-size: 12.5px; color: var(--muted); margin-top: 2px; line-height: 1.45; }
  .cap { text-transform: capitalize; }
  .tag { font-size: 10.5px; font-weight: 600; color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); padding: 1px 7px; border-radius: 999px; }

  .option { justify-content: flex-start; }
  .option:hover { background: var(--hover); }
  .radio { flex: none; width: 16px; height: 16px; border-radius: 50%; border: 1.5px solid var(--faint); }
  .option.sel .radio { border: 5px solid var(--accent); }
  .option.sel .radio.danger { border-color: var(--danger); }

  .btn { flex: none; height: 32px; padding: 0 14px; border-radius: 9px; border: 1px solid var(--border); font-size: 13px; font-weight: 500; }
  .btn:hover { background: var(--hover); }
  .btn.path { max-width: 280px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-family: var(--mono); font-size: 12px; }
  .btn.primary { background: var(--text); color: var(--bg); border-color: transparent; }
  .btn.danger:hover { color: var(--danger); }

  .seg { flex: none; display: flex; padding: 3px; border-radius: 10px; background: var(--raised); }
  .seg button { height: 28px; padding: 0 12px; border-radius: 8px; font-size: 12.5px; color: var(--muted); }
  .seg button.sel { background: var(--hover); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.4); }

  .keys kbd { font: 12px var(--font); color: var(--muted); background: var(--raised); border: 1px solid var(--border); padding: 1px 8px; border-radius: 6px; }

  .who { display: flex; align-items: center; gap: 12px; min-width: 0; flex: 1; }
  .accounts { display: grid; gap: 10px; }
  .tag.live { color: #4ade80; background: rgb(74 222 128 / 0.12); }
  .remote-err { margin-top: 10px; color: var(--danger); font-size: 13px; }
  .pair { margin-top: 16px; display: flex; gap: 24px; padding: 20px; border: 1px solid var(--border); border-radius: 14px; background: var(--panel); }
  .qr { flex: none; width: 196px; height: 196px; padding: 8px; border-radius: 12px; background: #fff; }
  .qr :global(svg) { width: 100%; height: 100%; display: block; }
  .pair-text { min-width: 0; flex: 1; }
  .pair-text ol { margin: 10px 0 14px; padding-left: 18px; color: var(--muted); font-size: 13px; line-height: 1.7; }
  .pair-text b { color: var(--text); font-weight: 550; }
  .mono { font-family: var(--mono); }
  .token-row { display: flex; align-items: center; gap: 6px; margin-top: 10px; flex-wrap: wrap; }
  .token { font: 11.5px var(--mono); color: var(--muted); background: var(--raised); padding: 5px 8px; border-radius: 6px; max-width: 100%; overflow: hidden; text-overflow: ellipsis; }
  .btn.small { height: 28px; padding: 0 10px; font-size: 12px; }
  .acard { border: 1px solid var(--border); border-radius: 14px; background: var(--panel); }
  .acard.active { border-color: color-mix(in srgb, var(--accent) 45%, var(--border)); }
  .btns { flex: none; display: flex; gap: 6px; }
  .btn.icon { width: 32px; padding: 0; display: grid; place-items: center; color: var(--muted); }
  .tag.muted { color: var(--muted); background: var(--raised); }
  .avatar.off { background: var(--raised); color: var(--faint); }
  .usage-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 18px; padding: 0 16px 14px 64px; }
  .u-top { display: flex; justify-content: space-between; font-size: 12px; color: var(--muted); margin-bottom: 5px; font-variant-numeric: tabular-nums; }
  .u .bar { height: 5px; }
  .u .hint { font-size: 11.5px; margin-top: 5px; }
  .add {
    margin-top: 12px; width: 100%; height: 44px; border-radius: 12px; border: 1px dashed var(--border);
    display: flex; align-items: center; justify-content: center; gap: 8px; color: var(--muted); font-size: 13.5px; font-weight: 500;
  }
  .add:hover:not(:disabled) { color: var(--text); background: var(--hover); border-style: solid; }
  .add:disabled { opacity: 0.7; }
  .avatar { width: 36px; height: 36px; border-radius: 50%; flex: none; display: grid; place-items: center; background: var(--accent); color: var(--accent-ink); font-weight: 650; }
  .bar { flex: 1; height: 6px; border-radius: 6px; background: var(--raised); overflow: hidden; }
  .bar > div { height: 100%; background: var(--accent); border-radius: 6px; }
  .bar > div.hot { background: var(--danger); }
</style>
