// Client for the hrns desktop remote API (served over Tailscale only).

export type Server = { url: string; token: string };

export type Part =
  | { type: "text" | "reasoning" | "plan"; id: string; text: string }
  | { type: "command"; id: string; command: string; actions?: { type: string; name?: string; path?: string | null; query?: string | null; command: string }[] | null; cwd?: string; output?: string | null; exitCode?: number | null; status: string }
  | { type: "fileChange"; id: string; changes: { path: string; kind: string; diff: string }[]; status: string }
  | { type: "tool"; id: string; title: string; status: string; error?: string | null };

export type Approval = { key: string; kind: "command" | "fileChange" | "permissions"; command?: string | null; cwd?: string | null; reason?: string | null; changes?: { path: string; kind: string }[] | null };

export type Message = { id: string; role: "user" | "assistant"; text: string; parts?: Part[]; model?: string; error?: string; createdAt: number };

export type Thread = { id: string; title: string; model: string; effort?: string | null; access: string; cwd?: string | null; account?: string | null; messages: Message[]; updatedAt: number };

export type Summary = {
  id: string; title: string; cwd?: string | null; account?: string | null; model: string; preview: string;
  messages: number; files: number; additions: number; deletions: number; commands: number; failed: boolean; updatedAt: number;
  usage: Record<string, { input: number; cached: number; output: number; reasoning: number }>;
};

export type Live = { parts: Part[]; approvals: Approval[]; notice?: string };

export type Model = { id: string; displayName: string; description: string; isDefault: boolean; defaultReasoningEffort: string; supportedReasoningEfforts: { reasoningEffort: string; description: string }[] };
export type RateWindow = { usedPercent: number; windowDurationMins: number; resetsAt: number } | null;
export type AccountStatus = { account: { email?: string; planType?: string } | null; rateLimits: { primary: RateWindow; secondary: RateWindow } | null };
export type UsageRow = { hour: string; source: string; model: string; input: number; cached: number; output: number; reasoning: number; requests: number };
export type Price = { input?: number | null; output?: number | null; cached?: number | null };
export type Prefs = { model?: string; effort?: string; access?: "read-only" | "auto" | "full"; account?: string; titleModel?: string };
export type Project = { path: string; modified: number };
export type ProjectIcon = { image: string | null; tech: string | null };
export type GitFile = { path: string; old: string | null; kind: "add" | "delete" | "rename" | "update"; add: number; del: number; binary: boolean; untracked: boolean };
export type GitStatus = { root: string; rev: string; files: GitFile[]; add: number; del: number };

export type ChatEvent =
  | { kind: "part"; threadId: string; part: Part }
  | { kind: "delta"; threadId: string; partId: string; field: "text" | "output"; text: string }
  | { kind: "notice"; threadId: string; message: string }
  | { kind: "approval"; threadId: string; approval: Approval }
  | { kind: "approvalResolved"; threadId: string; key: string }
  | { kind: "done"; threadId: string; message: Message | null };

export type ServerEvent =
  | { type: "chat"; payload: ChatEvent }
  | { type: "notify"; payload: { title: string; body: string; threadId?: string } }
  | { type: "thread-title"; payload: { threadId: string; title: string } }
  | { type: "resync" }
  | { type: "connection"; connected: boolean };

/** Parse the QR payload: hrns://pair?url=http://100.x.y.z:7788&token=… */
export function parsePairing(data: string): Server | null {
  const m = data.match(/^hrns:\/\/pair\?(.*)$/);
  if (!m) return null;
  const q = new Map(m[1].split("&").map((kv) => kv.split("=").map(decodeURIComponent) as [string, string]));
  const url = q.get("url");
  const token = q.get("token");
  return url && token && /^https?:\/\//.test(url) ? { url: url.replace(/\/$/, ""), token } : null;
}

export class Client {
  private ws: WebSocket | null = null;
  private listeners = new Set<(e: ServerEvent) => void>();
  private retry: ReturnType<typeof setTimeout> | null = null;
  private closed = false;
  connected = false;

  constructor(readonly server: Server) {}

  private async req<T>(path: string, init?: RequestInit, timeout = 15000): Promise<T> {
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), timeout);
    try {
      const res = await fetch(this.server.url + path, {
        ...init,
        signal: ctrl.signal,
        headers: { Authorization: `Bearer ${this.server.token}`, "Content-Type": "application/json", ...(init?.headers ?? {}) },
      });
      const body = await res.text();
      if (res.status === 401) throw new Error("Pairing expired — scan the QR code in hrns again.");
      if (!res.ok) throw new Error(safeError(body) ?? `Request failed (${res.status})`);
      return JSON.parse(body) as T;
    } catch (e) {
      if ((e as Error).name === "AbortError") throw new Error("Can't reach your Mac. Is Tailscale on, and hrns running?");
      throw e;
    } finally {
      clearTimeout(timer);
    }
  }

  info = () => this.req<{ name: string; host?: string; version: string; accounts: { id: string; email?: string | null }[] }>("/api/info");
  threads = () => this.req<Summary[]>("/api/threads");
  thread = (id: string) => this.req<{ thread: Thread; live: Live | null }>(`/api/threads/${encodeURIComponent(id)}`);
  models = (account = "default") => this.req<Model[]>(`/api/models?account=${encodeURIComponent(account)}`);
  accountStatus = (id: string) => this.req<AccountStatus>(`/api/accounts/${encodeURIComponent(id)}/status`);
  usage = (days: number) => this.req<UsageRow[]>(`/api/usage?days=${days}`);
  pricing = (models: string[]) => this.req<Record<string, Price>>("/api/pricing", { method: "POST", body: JSON.stringify({ models }) });
  send = (body: { threadId?: string; text: string; cwd?: string | null } & Prefs) => this.req<Thread>("/api/send", { method: "POST", body: JSON.stringify(body) });
  stop = (threadId: string) => this.req("/api/stop", { method: "POST", body: JSON.stringify({ threadId }) });
  // Walks the Mac's code folders; can take a while on a big home folder.
  projects = () => this.req<Project[]>("/api/projects", undefined, 60000);
  projectIcon = (path: string) => this.req<ProjectIcon>(`/api/projects/icon?path=${encodeURIComponent(path)}`);
  /** null when the folder isn't inside a git repository. */
  gitStatus = (path: string) => this.req<GitStatus | null>(`/api/git/status?path=${encodeURIComponent(path)}`);
  gitDiff = (root: string, f: GitFile) =>
    this.req<{ diff: string | null }>(
      `/api/git/diff?root=${encodeURIComponent(root)}&path=${encodeURIComponent(f.path)}${f.old ? `&old=${encodeURIComponent(f.old)}` : ""}&untracked=${f.untracked}`,
    ).then((r) => r.diff);
  title = (threadId: string, model: string, effort: string | null) =>
    this.req<{ title: string }>("/api/title", { method: "POST", body: JSON.stringify({ threadId, model, effort }) }, 60000).then((r) => r.title);
  approve = (key: string, decision: "accept" | "acceptForSession" | "decline") =>
    this.req("/api/approval", { method: "POST", body: JSON.stringify({ key, decision }) });

  on(fn: (e: ServerEvent) => void) {
    this.listeners.add(fn);
    return () => void this.listeners.delete(fn);
  }

  private emit(e: ServerEvent) {
    for (const fn of this.listeners) fn(e);
  }

  /** Live events with automatic reconnect (resync after each reconnect). */
  connect() {
    this.closed = false;
    const wsUrl = this.server.url.replace(/^http/, "ws") + `/api/events?token=${encodeURIComponent(this.server.token)}`;
    const ws = new WebSocket(wsUrl);
    this.ws = ws;
    ws.onopen = () => {
      this.connected = true;
      this.emit({ type: "connection", connected: true });
      this.emit({ type: "resync" });
    };
    ws.onmessage = (m) => {
      try {
        this.emit(JSON.parse(String(m.data)));
      } catch {}
    };
    ws.onclose = () => {
      if (this.connected) this.emit({ type: "connection", connected: false });
      this.connected = false;
      if (!this.closed) this.retry = setTimeout(() => this.connect(), 2000);
    };
    ws.onerror = () => ws.close();
  }

  close() {
    this.closed = true;
    if (this.retry) clearTimeout(this.retry);
    this.ws?.close();
  }
}

function safeError(body: string): string | null {
  try {
    return JSON.parse(body).error ?? null;
  } catch {
    return body || null;
  }
}

// ---------- display helpers (mirrors the desktop app) ----------

export function cleanCommand(cmd: string): string {
  let c = cmd.trim();
  const shell = c.match(/^(?:\/\S*\/)?(?:ba|z)?sh\s+-l?c\s+([\s\S]+)$/);
  if (shell) {
    c = shell[1].trim();
    const q = c[0];
    if ((q === '"' || q === "'") && c.endsWith(q)) c = c.slice(1, -1);
  }
  return c.replace(/(^|[|;&]\s*)rtk(?:\s+proxy)?\s+/g, "$1").trim();
}

export function describeCommand(p: Extract<Part, { type: "command" }>): string {
  const a = (p.actions ?? []).filter(Boolean);
  if (a.length && a.every((x) => x.type !== "unknown")) {
    return a
      .map((x) =>
        x.type === "read" ? `Read ${x.name ?? base(x.path)}` : x.type === "listFiles" ? `Listed ${base(x.path) || "."}` : `Searched ${x.query ? `“${x.query}”` : cleanCommand(x.command)}`,
      )
      .join(" · ");
  }
  return `Ran ${cleanCommand(p.command)}`;
}

export function diffStat(c: { kind: string; diff: string }) {
  const lines = c.diff.replace(/\n$/, "").split("\n");
  if (c.kind === "add") return { add: lines.length, del: 0 };
  if (c.kind === "delete") return { add: 0, del: lines.length };
  let add = 0, del = 0;
  for (const l of lines) {
    if (l.startsWith("+") && !l.startsWith("+++")) add++;
    else if (l.startsWith("-") && !l.startsWith("---")) del++;
  }
  return { add, del };
}

export const base = (p?: string | null) => (p ? p.split("/").filter(Boolean).pop() ?? p : "");

export function ago(ts: number) {
  const m = Math.round((Date.now() - ts) / 60_000);
  if (m < 1) return "now";
  if (m < 60) return `${m}m`;
  if (m < 1440) return `${Math.round(m / 60)}h`;
  return `${Math.round(m / 1440)}d`;
}

export const tokens = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : String(n));
