import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open as openDialog, message } from "@tauri-apps/plugin-dialog";

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

export type Theme = "system" | "light" | "dark";

/** Interface and code fonts. Only faces that ship with macOS, so every choice renders. */
export const fonts = {
  system: { label: "System", stack: '-apple-system, BlinkMacSystemFont, "SF Pro Text", "Inter", "Segoe UI", sans-serif' },
  rounded: { label: "Rounded", stack: 'ui-rounded, "SF Pro Rounded", -apple-system, sans-serif' },
  serif: { label: "New York", stack: 'ui-serif, "New York", Georgia, serif' },
  avenir: { label: "Avenir Next", stack: '"Avenir Next", Avenir, -apple-system, sans-serif' },
  helvetica: { label: "Helvetica", stack: '"Helvetica Neue", Helvetica, Arial, sans-serif' },
} as const;
export const monoFonts = {
  sf: { label: "SF Mono", stack: 'ui-monospace, "SF Mono", "JetBrains Mono", Menlo, monospace' },
  menlo: { label: "Menlo", stack: "Menlo, ui-monospace, monospace" },
  monaco: { label: "Monaco", stack: "Monaco, ui-monospace, monospace" },
  courier: { label: "Courier", stack: '"Courier New", Courier, monospace' },
} as const;
/** Accent presets, each tuned per theme (lighter on dark, deeper on light). Custom colors are "#rrggbb". */
export const accents = {
  violet: { label: "Violet", dark: "#a78bfa", light: "#7c3aed" },
  blue: { label: "Blue", dark: "#60a5fa", light: "#2563eb" },
  teal: { label: "Teal", dark: "#2dd4bf", light: "#0d9488" },
  green: { label: "Green", dark: "#4ade80", light: "#16a34a" },
  amber: { label: "Amber", dark: "#fbbf24", light: "#d97706" },
  orange: { label: "Orange", dark: "#fb923c", light: "#ea580c" },
  rose: { label: "Rose", dark: "#fb7185", light: "#e11d48" },
  pink: { label: "Pink", dark: "#f472b6", light: "#db2777" },
} as const;
export type AccentPreset = keyof typeof accents;
export type Font = keyof typeof fonts;
export type Project = { path: string; modified: number };
/** A chat's own git worktree: `cwd` is its folder, `root` the project's main checkout. */
export type Worktree = { root: string; branch: string; base: string };
export type RepoInfo = { root: string; current: string | null; branches: string[] };
/** The project a chat belongs to (its main checkout, for chats in a worktree). */
export const projectOf = (t: { cwd?: string | null; worktree?: Worktree | null }) => t.worktree?.root ?? t.cwd ?? null;
export type MonoFont = keyof typeof monoFonts;

export type Message = {
  id: string;
  role: "user" | "assistant";
  text: string;
  parts?: Part[];
  model?: string;
  error?: string;
  /** Files attached to a user message (absolute paths). */
  attachments?: string[];
  createdAt: number;
};

export type Thread = {
  id: string;
  title: string;
  model: string;
  effort?: string | null;
  access: Access;
  cwd?: string | null;
  worktree?: Worktree | null;
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
  worktree?: Worktree | null;
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

/** One chat column. The window shows one, or two side by side (split view). */
export class Pane {
  current = $state<Thread | null>(null);
  live = $state<Live | null>(null);
}

class AppState {
  ready = $state(false);
  threads = $state<Summary[]>([]);
  panes = $state<Pane[]>([new Pane()]);
  /** Index of the pane that the sidebar, shortcuts and settings act on. */
  focused = $state(0);
  /** Width of the left pane in split view, as a fraction of the main area. */
  splitRatio = $state(Number(saved("hrns.split")) || 0.5);
  /** The changes sidebar (uncommitted git changes of the focused pane's project). */
  diffOpen = $state(saved("hrns.diffOpen") === "1");
  diffWidth = $state(Number(saved("hrns.diffWidth")) || 440);
  streaming = $state<Record<string, true>>({});
  /** Threads with an approval waiting on the user. */
  waiting = $state<Record<string, true>>({});
  models = $state<Model[]>([]);
  model = $state<string>(saved("hrns.model") ?? "");
  effort = $state<string | null>(saved("hrns.effort"));
  /** Who names new chats: "auto" (a fast model), "off" (first line of the message), or a model id. */
  titleModel = $state<string>(saved("hrns.titleModel") ?? "auto");
  access = $state<Access>((saved("hrns.access") as Access) ?? "auto");
  /** Project folder for the next new chat. */
  folder = $state<string | null>(saved("hrns.folder"));
  /** Where the next new chat works: in the project folder, or in a new worktree of it. */
  workspace = $state<"local" | "worktree">(saved("hrns.workspace") === "worktree" ? "worktree" : "local");
  /** Git info of `folder` (null when it isn't a repo), and the branch a new worktree starts from. */
  repo = $state.raw<RepoInfo | null>(null);
  base = $state<string | null>(null);
  /** Extra models that get the same first message, each in its own worktree (compare mode). */
  compare = $state<string[]>([]);
  /** Open project finder; resolves the pending `pickFolder()` call. */
  finder = $state.raw<{ resolve: (dir: string | null) => void } | null>(null);
  /** Project folders found on disk (scanned each time the finder opens). */
  projects = $state.raw<Project[]>([]);
  scanning = $state(false);
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
  /** Chat column: a readable centered column, or the full width of the pane. */
  chatWidth = $state<"default" | "full">(saved("hrns.chatWidth") === "full" ? "full" : "default");
  textSize = $state<"small" | "default" | "large">((saved("hrns.textSize") as "small" | "default" | "large") ?? "default");
  theme = $state<Theme>((saved("hrns.theme") as Theme) ?? "system");
  /** A preset name or a custom "#rrggbb". */
  accent = $state<string>(saved("hrns.accent") ?? "violet");
  font = $state<Font>((saved("hrns.font") as Font) in fonts ? (saved("hrns.font") as Font) : "system");
  monoFont = $state<MonoFont>((saved("hrns.monoFont") as MonoFont) in monoFonts ? (saved("hrns.monoFont") as MonoFont) : "sf");

  get pane(): Pane {
    return this.panes[this.focused] ?? this.panes[0];
  }
  get current() {
    return this.pane.current;
  }
  set current(t: Thread | null) {
    this.pane.current = t;
  }
  get live() {
    return this.pane.live;
  }
  set live(l: Live | null) {
    this.pane.live = l;
  }

  /** Whether a thread is showing in any pane. */
  isOpen(id: string) {
    return this.panes.some((p) => p.current?.id === id);
  }

  focus(pane: Pane) {
    const i = this.panes.indexOf(pane);
    if (i < 0 || i === this.focused) return;
    this.focused = i;
    if (pane.current) this.access = pane.current.access ?? "auto";
  }

  /** Open a thread beside the current one (or into a side of an existing split). */
  async openSplit(id: string, side: "left" | "right") {
    this.view = "chat";
    if (this.panes.length === 1) {
      if (this.panes[0].current?.id === id) return;
      const pane = new Pane();
      this.panes = side === "left" ? [pane, this.panes[0]] : [this.panes[0], pane];
      this.focused = this.panes.indexOf(pane);
      await this.open(id, pane);
      return;
    }
    const target = this.panes[side === "left" ? 0 : 1];
    const other = this.panes[side === "left" ? 1 : 0];
    if (other.current?.id === id) {
      // Dropped onto the opposite side: swap the two chats.
      this.panes = [this.panes[1], this.panes[0]];
      this.focused = this.panes.indexOf(other);
      return;
    }
    this.focus(target);
    await this.open(id, target);
  }

  closePane(pane: Pane) {
    if (this.panes.length < 2) return;
    this.panes = this.panes.filter((p) => p !== pane);
    this.focused = 0;
    if (this.current) this.access = this.current.access ?? "auto";
  }

  setDiffOpen(open: boolean) {
    this.diffOpen = open;
    save("hrns.diffOpen", open ? "1" : "0");
  }

  setDiffWidth(w: number) {
    this.diffWidth = Math.round(Math.min(900, Math.max(300, w)));
    save("hrns.diffWidth", String(this.diffWidth));
  }

  setSplitRatio(r: number) {
    this.splitRatio = Math.min(0.75, Math.max(0.25, r));
    save("hrns.split", String(this.splitRatio));
  }

  get currentModel(): Model | undefined {
    return this.models.find((m) => m.id === this.model);
  }

  async init() {
    listen<ChatEvent>("chat", (e) => this.onChat(e.payload));
    listen<{ threadId: string; title: string }>("thread-title", ({ payload: { threadId, title } }) => {
      for (const p of this.panes) if (p.current?.id === threadId) p.current.title = title;
      const s = this.threads.find((t) => t.id === threadId);
      if (s) s.title = title;
    });
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

  setTitleModel(id: string) {
    this.titleModel = id;
    save("hrns.titleModel", id);
  }

  /** The model that names chats, or null when naming is off. "auto" prefers a small, fast model. */
  get titleNamer(): Model | null {
    if (this.titleModel === "off") return null;
    return (
      this.models.find((m) => m.id === this.titleModel) ??
      this.models.find((m) => /mini|nano|spark|flash|lite/i.test(m.id)) ??
      this.currentModel ??
      null
    );
  }

  /** Ask the naming model for a title; the result arrives as a `thread-title` event. */
  async nameThread(id: string) {
    const m = this.titleNamer;
    if (!m) return;
    // Naming needs no thinking: use the lightest effort the model offers.
    const effort = ["none", "minimal", "low"].find((e) => m.supportedReasoningEfforts.some((s) => s.reasoningEffort === e)) ?? null;
    try {
      await invoke("generate_title", { threadId: id, model: m.id, effort });
    } catch {
      // Keep the first-line title.
    }
  }

  setAccess(access: Access) {
    this.access = access;
    save("hrns.access", access);
  }

  /** Opens the project finder; resolves with the chosen folder (also made the default), or null. */
  pickFolder(): Promise<string | null> {
    this.finder?.resolve(null);
    this.scanProjects();
    return new Promise((resolve) => {
      const finder = {
        resolve: (dir: string | null) => {
          if (this.finder === finder) this.finder = null;
          if (dir) {
            this.folder = dir;
            save("hrns.folder", dir);
          }
          resolve(dir);
        },
      };
      this.finder = finder;
    });
  }

  /** The native folder dialog, for folders the finder doesn't know about. */
  async browseFolder(): Promise<string | null> {
    const dir = await openDialog({ directory: true, defaultPath: this.folder ?? undefined, title: "Choose a project folder" });
    return typeof dir === "string" ? dir : null;
  }

  async scanProjects() {
    if (this.scanning) return;
    this.scanning = true;
    try {
      this.projects = await invoke<Project[]>("find_projects");
    } catch {
    } finally {
      this.scanning = false;
    }
  }

  toggleCompare(id: string) {
    this.compare = this.compare.includes(id) ? this.compare.filter((m) => m !== id) : [...this.compare, id];
  }

  /** One extra chat of a comparison: same message, another model, its own worktree. */
  private async sendExtra(text: string, model: string, base: string, attachments: string[]): Promise<string | null> {
    const id = crypto.randomUUID();
    const m = this.models.find((x) => x.id === model);
    const effort = this.effort && m?.supportedReasoningEfforts.some((e) => e.reasoningEffort === this.effort) ? this.effort : null;
    buffers.set(id, { ...emptyLive(), notice: `Creating a worktree from ${base}…` });
    this.streaming[id] = true;
    try {
      await invoke<Thread>("send_message", {
        threadId: id, text, model, effort, access: this.access, cwd: this.folder, account: this.activeAccount, worktree: base, attachments,
      });
      await this.refreshThreads();
      this.nameThread(id);
      return id;
    } catch (e) {
      delete this.streaming[id];
      buffers.delete(id);
      this.error = `${this.modelName(model)}: ${e}`;
      return null;
    }
  }

  setWorkspace(w: "local" | "worktree", base?: string) {
    this.workspace = w;
    save("hrns.workspace", w);
    if (base) this.base = base;
  }

  /** Branches of the selected project, for the worktree picker. */
  async loadRepo(dir = this.folder) {
    let info: RepoInfo | null = null;
    try {
      if (dir) info = await invoke<RepoInfo | null>("repo_info", { path: dir });
    } catch {}
    if (dir !== this.folder) return;
    this.repo = info;
    if (!info) this.base = null;
    else if (!this.base || !info.branches.includes(this.base)) this.base = info.current ?? info.branches[0] ?? null;
  }

  newChatIn(dir: string) {
    this.folder = dir;
    save("hrns.folder", dir);
    this.newChat();
  }

  async respond(key: string, decision: "accept" | "acceptForSession" | "decline" | "cancel") {
    for (const p of this.panes) if (p.live) p.live.approvals = p.live.approvals.filter((a) => a.key !== key);
    await invoke("respond_approval", { key, decision });
  }

  setChatWidth(w: "default" | "full") {
    this.chatWidth = w;
    save("hrns.chatWidth", w);
  }

  setTextSize(size: "small" | "default" | "large") {
    this.textSize = size;
    save("hrns.textSize", size);
  }

  setTheme(theme: Theme) {
    this.theme = theme;
    save("hrns.theme", theme);
  }

  setFont(font: Font) {
    this.font = font;
    save("hrns.font", font);
  }

  setAccent(accent: string) {
    this.accent = accent;
    save("hrns.accent", accent);
  }

  setMonoFont(font: MonoFont) {
    this.monoFont = font;
    save("hrns.monoFont", font);
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

  async open(id: string, pane = this.pane) {
    this.view = "chat";
    if (pane.current?.id === id) return;
    // Already showing in the other pane: just move focus there.
    const shown = this.panes.find((p) => p !== pane && p.current?.id === id);
    if (shown) return this.focus(shown);
    const r = await invoke<[Thread, Live | null] | null>("get_thread", { id });
    if (!r) return;
    const [thread, live] = r;
    pane.current = thread;
    if (pane === this.pane) this.access = thread.access ?? "auto";
    if (live) buffers.set(id, live);
    pane.live = live;
  }

  async remove(id: string) {
    const s = this.threads.find((t) => t.id === id);
    let removeWorktree = false;
    if (s?.worktree) {
      const others = this.threads.some((t) => t.id !== id && t.cwd === s.cwd);
      const c = others ? null : await invoke<{ exists: boolean; dirty: number; ahead: number } | null>("worktree_check", { id }).catch(() => null);
      if (c?.exists) {
        const lose = [
          c.dirty && `${c.dirty} uncommitted file${c.dirty > 1 ? "s" : ""} will be lost`,
          c.ahead && `branch ${s.worktree.branch} has ${c.ahead} commit${c.ahead > 1 ? "s" : ""} not on ${s.worktree.base} and will be kept`,
        ].filter(Boolean);
        const answer = await message(
          `This chat works in its own worktree on ${s.worktree.branch}.` + (lose.length ? `\n\nIf you remove it: ${lose.join("; ")}.` : ""),
          { title: `Delete “${s.title}”?`, kind: "warning", buttons: { yes: "Delete chat and worktree", no: "Delete chat only", cancel: "Cancel" } },
        );
        if (answer === "Cancel" || answer === "cancel") return;
        removeWorktree = answer === "Yes" || answer === "Delete chat and worktree";
      }
    }
    try {
      await invoke("delete_thread", { id, removeWorktree });
    } catch (e) {
      this.error = String(e);
    }
    this.threads = this.threads.filter((t) => t.id !== id);
    buffers.delete(id);
    delete this.streaming[id];
    delete this.waiting[id];
    for (const p of this.panes) {
      if (p.current?.id !== id) continue;
      if (this.panes.length > 1) this.closePane(p);
      else this.newChat();
    }
  }

  async send(text: string, pane = this.pane, attachments: string[] = []) {
    text = text.trim();
    if (!text && attachments.length) text = "See the attached file" + (attachments.length > 1 ? "s." : ".");
    if (!text || !this.model) return;
    if (!pane.current && !this.folder && !(await this.pickFolder())) return;
    const now = Date.now();
    const isNew = !pane.current;
    const id = pane.current?.id ?? crypto.randomUUID();
    const userMsg: Message = { id: crypto.randomUUID(), role: "user", text, createdAt: now, ...(attachments.length ? { attachments } : {}) };

    if (pane.current) {
      pane.current.messages.push(userMsg);
    } else {
      pane.current = { id, title: text.slice(0, 48), model: this.model, effort: this.effort, access: this.access, cwd: this.folder, createdAt: now, updatedAt: now, messages: [userMsg] };
    }
    // Compare mode: the same message to other models too, every chat in its own worktree.
    const extras = isNew && this.repo && this.base ? this.compare.filter((m) => m !== this.model && this.models.some((x) => x.id === m)) : [];
    // New chat in a new worktree of a git project, branched from `base`.
    const base = isNew && (this.workspace === "worktree" || extras.length > 0) && this.repo && this.base ? this.base : null;
    if (extras.length && base) {
      this.compare = [];
      Promise.all(extras.map((m) => this.sendExtra(text, m, base, attachments))).then((ids) => {
        // Two models: show them side by side.
        const other = ids.find(Boolean);
        if (ids.length === 1 && other && this.panes.length === 1) this.openSplit(other, "right");
      });
    }
    const live = emptyLive();
    if (base) live.notice = `Creating a worktree from ${base}…`;
    buffers.set(id, live);
    this.streaming[id] = true;
    pane.live = { ...live };

    try {
      const thread = await invoke<Thread>("send_message", {
        threadId: id, text, model: this.model, effort: this.effort, access: this.access, cwd: pane.current?.cwd ?? this.folder,
        account: this.activeAccount, worktree: base, attachments,
      });
      for (const p of this.panes) {
        if (p.current?.id !== id) continue;
        p.current.cwd = thread.cwd;
        p.current.worktree = thread.worktree;
      }
      this.refreshThreads();
      if (isNew) this.nameThread(id);
    } catch (e) {
      this.onChat({ kind: "done", threadId: id, message: { id: crypto.randomUUID(), role: "assistant", text: "", error: String(e), createdAt: Date.now() } });
    }
  }

  async stop(pane = this.pane) {
    if (pane.current) await invoke("stop_turn", { threadId: pane.current.id });
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
        for (const p of this.panes) {
          const cur = p.current;
          if (cur?.id !== ev.threadId) continue;
          if (ev.message && !cur.messages.some((m) => m.id === ev.message!.id)) cur.messages.push(ev.message);
          p.live = null;
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
    if (this.isOpen(ev.threadId) && !flushTimer) {
      flushTimer = setTimeout(() => {
        flushTimer = null;
        for (const pane of this.panes) {
          const b = pane.current && buffers.get(pane.current.id);
          if (b) pane.live = { parts: b.parts.map((p) => ({ ...p })), approvals: [...b.approvals], notice: b.notice };
        }
      }, FLUSH_MS);
    }
  }
}

export const app = new AppState();
