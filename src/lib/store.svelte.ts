import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

export type Part =
  | { type: "text" | "reasoning" | "plan"; id: string; text: string }
  | { type: "command"; id: string; command: string; actions?: CommandAction[] | null; cwd?: string; output?: string | null; exitCode?: number | null; durationMs?: number | null; status: string }
  | { type: "fileChange"; id: string; changes: { path: string; kind: string; diff: string }[]; status: string }
  | { type: "tool"; id: string; title: string; status: string; error?: string | null };

export type CommandAction = { type: "read" | "listFiles" | "search" | "unknown"; command: string; name?: string; path?: string | null; query?: string | null };

export type Approval = {
  key: string;
  kind: "command" | "fileChange" | "permissions";
  command?: string | null;
  cwd?: string | null;
  reason?: string | null;
  grantRoot?: string | null;
  changes?: { path: string; kind: string; diff: string }[] | null;
};

export type Access = "read-only" | "auto" | "full";

export type Message = {
  id: string;
  role: "user" | "assistant";
  text: string;
  parts?: Part[];
  model?: string;
  error?: string;
  createdAt: number;
};

export type Thread = {
  id: string;
  title: string;
  model: string;
  effort?: string | null;
  access: Access;
  cwd?: string | null;
  account?: string | null;
  createdAt: number;
  updatedAt: number;
  messages: Message[];
};

export type Usage = { input: number; cached: number; output: number; reasoning: number };
export type Price = { input?: number; output?: number; cached?: number };

export type Summary = {
  id: string;
  title: string;
  cwd?: string | null;
  account?: string | null;
  model: string;
  preview: string;
  messages: number;
  files: number;
  additions: number;
  deletions: number;
  commands: number;
  usage: Record<string, Usage>;
  failed: boolean;
  updatedAt: number;
};

export type Model = {
  id: string;
  model: string;
  displayName: string;
  description: string;
  supportedReasoningEfforts: { reasoningEffort: string; description: string }[];
  defaultReasoningEffort: string;
  isDefault: boolean;
};

export type RateWindow = { usedPercent: number; windowDurationMins: number; resetsAt: number } | null;
export type RateLimits = { primary: RateWindow; secondary: RateWindow } | null;
export type Account = { type: string; email?: string; planType?: string } | null;
export type AccountEntry = { id: string; email?: string | null; plan?: string | null };
export type AccountStatus = { account: Account; rateLimits: RateLimits };

export type Live = { parts: Part[]; approvals: Approval[]; notice?: string };
const emptyLive = (): Live => ({ parts: [], approvals: [] });

type ChatEvent =
  | { kind: "part"; threadId: string; part: Part }
  | { kind: "delta"; threadId: string; partId: string; field: "text" | "output"; text: string }
  | { kind: "notice"; threadId: string; message: string }
  | { kind: "approval"; threadId: string; approval: Approval }
  | { kind: "approvalResolved"; threadId: string; key: string }
  | { kind: "done"; threadId: string; message: Message | null };

const saved = (k: string) => {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
};
const save = (k: string, v: string | null) => {
  try {
    v == null ? localStorage.removeItem(k) : localStorage.setItem(k, v);
  } catch {}
};

// Streams for every thread (so switching threads mid-answer loses nothing).
// Not reactive on purpose: the visible one is flushed to `app.live` at most every FLUSH_MS.
const buffers = new Map<string, Live>();
const FLUSH_MS = 60;
let flushTimer: ReturnType<typeof setTimeout> | null = null;

class AppState {
  ready = $state(false);
  threads = $state<Summary[]>([]);
  current = $state<Thread | null>(null);
  live = $state<Live | null>(null);
  streaming = $state<Record<string, true>>({});
  /** Threads with an approval waiting on the user. */
  waiting = $state<Record<string, true>>({});
  models = $state<Model[]>([]);
  model = $state<string>(saved("hrns.model") ?? "");
  effort = $state<string | null>(saved("hrns.effort"));
  access = $state<Access>((saved("hrns.access") as Access) ?? "auto");
  /** Project folder for the next new chat. */
  folder = $state<string | null>(saved("hrns.folder"));
  /** Signed-in identity + usage of the active account. */
  account = $state<Account>(null);
  rateLimits = $state<RateLimits>(null);
  /** All connected ChatGPT accounts; new chats use `activeAccount`. */
  accounts = $state<AccountEntry[]>([]);
  activeAccount = $state<string>(saved("hrns.account") ?? "default");
  statuses = $state<Record<string, AccountStatus>>({});
  /** Account waiting for its browser sign-in to finish. */
  pendingLogin = $state<string | null>(null);
  error = $state<string | null>(null);
  /** USD per 1M tokens, keyed by model id (models.dev catalog). */
  pricing = $state<Record<string, Price>>({});
  sidebarOpen = $state(saved("hrns.sidebar") !== "0");
  view = $state<"chat" | "settings" | "usage">("chat");
  settingsSection = $state("general");
  textSize = $state<"small" | "default" | "large">((saved("hrns.textSize") as "small" | "default" | "large") ?? "default");

  get currentModel(): Model | undefined {
    return this.models.find((m) => m.id === this.model);
  }

  async init() {
    listen<ChatEvent>("chat", (e) => this.onChat(e.payload));
    listen<{ account: string; rateLimits: RateLimits }>("rate-limits", ({ payload: { account, rateLimits } }) => {
      this.statuses[account] = { account: this.statuses[account]?.account ?? null, rateLimits };
      if (account === this.activeAccount) this.rateLimits = rateLimits;
    });
    listen<{ account: string }>("account-changed", async ({ payload: { account } }) => {
      const s = await this.refreshStatus(account);
      if (this.pendingLogin === account && s?.account) {
        this.pendingLogin = null;
        this.setActiveAccount(account);
      } else if (account === this.activeAccount) {
        this.loadAccount();
      }
    });
    this.threads = await invoke<Summary[]>("list_threads");
    this.accounts = await invoke<AccountEntry[]>("accounts");
    if (!this.accounts.some((a) => a.id === this.activeAccount)) this.activeAccount = "default";
    await this.loadAccount();
    this.ready = true;
  }

  async loadAccount() {
    try {
      const r = await invoke<AccountStatus>("account_status", { account: this.activeAccount });
      this.statuses[this.activeAccount] = r;
      this.account = r.account;
      this.rateLimits = r.rateLimits;
      this.error = null;
      this.accounts = await invoke<AccountEntry[]>("accounts");
      if (r.account) await this.loadModels();
    } catch (e) {
      this.error = String(e);
    }
  }

  /** Live identity + usage for any account (starts its Codex process). */
  async refreshStatus(id: string): Promise<AccountStatus | null> {
    try {
      const r = await invoke<AccountStatus>("account_status", { account: id });
      this.statuses[id] = r;
      this.accounts = await invoke<AccountEntry[]>("accounts");
      return r;
    } catch {
      return null;
    }
  }

  setActiveAccount(id: string) {
    this.activeAccount = id;
    save("hrns.account", id);
    this.account = this.statuses[id]?.account ?? null;
    this.rateLimits = this.statuses[id]?.rateLimits ?? null;
    this.loadAccount();
  }

  accountLabel(id: string | null | undefined) {
    const a = this.accounts.find((a) => a.id === (id ?? "default"));
    return a?.email ?? (a?.id === "default" ? "Default account" : "Not signed in");
  }

  async addAccount() {
    try {
      const id = await invoke<string>("add_account");
      this.accounts = await invoke<AccountEntry[]>("accounts");
      this.pendingLogin = id;
      await openUrl(await invoke<string>("login", { account: id }));
    } catch (e) {
      this.error = String(e);
      this.pendingLogin = null;
    }
  }

  async removeAccount(id: string) {
    await invoke("remove_account", { account: id });
    delete this.statuses[id];
    if (this.pendingLogin === id) this.pendingLogin = null;
    this.accounts = await invoke<AccountEntry[]>("accounts");
    if (this.activeAccount === id) this.setActiveAccount("default");
  }

  async loadModels() {
    this.models = await invoke<Model[]>("list_models", { account: this.activeAccount });
    this.loadPricing();
    if (!this.currentModel) {
      const m = this.models.find((m) => m.isDefault) ?? this.models[0];
      if (m) this.setModel(m.id);
    }
  }

  setModel(id: string) {
    this.model = id;
    save("hrns.model", id);
    const m = this.currentModel;
    if (m && this.effort && !m.supportedReasoningEfforts.some((e) => e.reasoningEffort === this.effort)) {
      this.setEffort(null);
    }
  }

  setEffort(effort: string | null) {
    this.effort = effort;
    save("hrns.effort", effort);
  }

  setAccess(access: Access) {
    this.access = access;
    save("hrns.access", access);
  }

  async pickFolder(): Promise<string | null> {
    const dir = await openDialog({ directory: true, defaultPath: this.folder ?? undefined, title: "Choose a project folder" });
    if (typeof dir !== "string") return null;
    this.folder = dir;
    save("hrns.folder", dir);
    return dir;
  }

  newChatIn(dir: string) {
    this.folder = dir;
    save("hrns.folder", dir);
    this.newChat();
  }

  async respond(key: string, decision: "accept" | "acceptForSession" | "decline" | "cancel") {
    if (this.live) this.live.approvals = this.live.approvals.filter((a) => a.key !== key);
    await invoke("respond_approval", { key, decision });
  }

  setTextSize(size: "small" | "default" | "large") {
    this.textSize = size;
    save("hrns.textSize", size);
  }

  openSettings(section?: string) {
    if (section) this.settingsSection = section;
    this.view = "settings";
  }

  toggleSidebar() {
    this.sidebarOpen = !this.sidebarOpen;
    save("hrns.sidebar", this.sidebarOpen ? "1" : "0");
  }

  async login(id = this.activeAccount) {
    try {
      this.pendingLogin = id;
      await openUrl(await invoke<string>("login", { account: id }));
    } catch (e) {
      this.error = String(e);
      this.pendingLogin = null;
    }
  }

  async logout(id = this.activeAccount) {
    await invoke("logout", { account: id });
    this.statuses[id] = { account: null, rateLimits: null };
    this.accounts = await invoke<AccountEntry[]>("accounts");
    if (id === this.activeAccount) {
      this.account = null;
      this.rateLimits = null;
      this.models = [];
    }
  }

  newChat() {
    this.view = "chat";
    this.current = null;
    this.live = null;
  }

  async open(id: string) {
    this.view = "chat";
    if (this.current?.id === id) return;
    const r = await invoke<[Thread, Live | null] | null>("get_thread", { id });
    if (!r) return;
    const [thread, live] = r;
    this.current = thread;
    this.access = thread.access ?? "auto";
    if (live) buffers.set(id, live);
    this.live = live;
  }

  async remove(id: string) {
    await invoke("delete_thread", { id });
    this.threads = this.threads.filter((t) => t.id !== id);
    buffers.delete(id);
    delete this.streaming[id];
    delete this.waiting[id];
    if (this.current?.id === id) this.newChat();
  }

  async send(text: string) {
    text = text.trim();
    if (!text || !this.model) return;
    if (!this.current && !this.folder && !(await this.pickFolder())) return;
    const now = Date.now();
    const id = this.current?.id ?? crypto.randomUUID();
    const userMsg: Message = { id: crypto.randomUUID(), role: "user", text, createdAt: now };

    if (this.current) {
      this.current.messages.push(userMsg);
    } else {
      this.current = { id, title: text.slice(0, 48), model: this.model, effort: this.effort, access: this.access, cwd: this.folder, createdAt: now, updatedAt: now, messages: [userMsg] };
    }
    buffers.set(id, emptyLive());
    this.streaming[id] = true;
    this.live = emptyLive();

    try {
      const thread = await invoke<Thread>("send_message", {
        threadId: id, text, model: this.model, effort: this.effort, access: this.access, cwd: this.current?.cwd ?? this.folder,
        account: this.activeAccount,
      });
      this.refreshThreads();
    } catch (e) {
      this.onChat({ kind: "done", threadId: id, message: { id: crypto.randomUUID(), role: "assistant", text: "", error: String(e), createdAt: Date.now() } });
    }
  }

  async stop() {
    if (this.current) await invoke("stop_turn", { threadId: this.current.id });
  }

  async refreshThreads() {
    this.threads = await invoke<Summary[]>("list_threads");
  }

  /** Prices for every model we show or have used (cached by the backend; fetched once). */
  async loadPricing(extra: string[] = []) {
    const ids = new Set([...this.models.map((m) => m.id), ...extra]);
    for (const id of Object.keys(this.pricing)) ids.add(id);
    for (const t of this.threads) for (const id of Object.keys(t.usage ?? {})) ids.add(id);
    try {
      this.pricing = await invoke<Record<string, Price>>("pricing", { models: [...ids] });
    } catch {}
  }

  /** Total tokens and API-equivalent cost (null if any model has no known price). */
  usageOf(s: Summary) {
    const total: Usage = { input: 0, cached: 0, output: 0, reasoning: 0 };
    let cost: number | null = 0;
    for (const [model, u] of Object.entries(s.usage ?? {})) {
      total.input += u.input;
      total.cached += u.cached;
      total.output += u.output;
      total.reasoning += u.reasoning;
      const p = this.pricing[model];
      if (!p?.input || !p.output) {
        if (u.input || u.output) cost = null;
        continue;
      }
      if (cost != null) cost += ((u.input - u.cached) * p.input + u.cached * (p.cached ?? p.input) + u.output * p.output) / 1e6;
    }
    return { ...total, tokens: total.input + total.output, cost };
  }

  modelName(id: string) {
    return this.models.find((m) => m.id === id)?.displayName ?? id;
  }

  private onChat(ev: ChatEvent) {
    const buf = buffers.get(ev.threadId) ?? emptyLive();
    buffers.set(ev.threadId, buf);
    switch (ev.kind) {
      case "part": {
        const i = buf.parts.findIndex((p) => p.id === ev.part.id);
        if (i >= 0) buf.parts[i] = ev.part;
        else buf.parts.push(ev.part);
        buf.notice = undefined;
        break;
      }
      case "delta": {
        let part = buf.parts.find((p) => p.id === ev.partId) as Record<string, any> | undefined;
        if (!part) {
          part = { type: ev.field === "output" ? "command" : "text", id: ev.partId, [ev.field]: "" };
          buf.parts.push(part as Part);
        }
        part[ev.field] = (part[ev.field] ?? "") + ev.text;
        buf.notice = undefined;
        break;
      }
      case "approval":
        buf.approvals.push(ev.approval);
        break;
      case "approvalResolved":
        buf.approvals = buf.approvals.filter((a) => a.key !== ev.key);
        break;
      case "notice":
        buf.notice = ev.message;
        break;
      case "done": {
        buffers.delete(ev.threadId);
        delete this.streaming[ev.threadId];
        const cur = this.current;
        if (cur?.id === ev.threadId) {
          if (ev.message && !cur.messages.some((m) => m.id === ev.message!.id)) cur.messages.push(ev.message);
          this.live = null;
        }
        delete this.waiting[ev.threadId];
        this.refreshThreads();
        return;
      }
    }
    if (ev.kind === "approval" || ev.kind === "approvalResolved") {
      if (buf.approvals.length) this.waiting[ev.threadId] = true;
      else delete this.waiting[ev.threadId];
    }
    if (this.current?.id === ev.threadId && !flushTimer) {
      flushTimer = setTimeout(() => {
        flushTimer = null;
        const b = this.current && buffers.get(this.current.id);
        if (b) this.live = { parts: b.parts.map((p) => ({ ...p })), approvals: [...b.approvals], notice: b.notice };
      }, FLUSH_MS);
    }
  }
}

export const app = new AppState();
