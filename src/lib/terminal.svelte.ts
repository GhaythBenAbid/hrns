import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// Terminal sessions live in the Rust core and are shared with paired phones, so this store
// only mirrors the list and remembers how the panel looks.

export type TermInfo = { id: string; cwd: string; title: string; alive: boolean; createdAt: number };

const saved = (k: string) => {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
};
const save = (k: string, v: string) => {
  try {
    localStorage.setItem(k, v);
  } catch {}
};

type Output = (data: string) => void;
const outputs = new Map<string, Output>();
const exits = new Map<string, () => void>();
/** Current PTY size per session ("80x24"), changed by any client. */
export const ptySize = new Map<string, string>();

class Terms {
  list = $state<TermInfo[]>([]);
  active = $state<string | null>(null);
  open = $state(saved("hrns.termOpen") === "1");
  height = $state(Number(saved("hrns.termHeight")) || 260);
  private started = false;

  async init() {
    if (this.started) return;
    this.started = true;
    await listen<TermInfo[]>("terminals", (e) => this.sync(e.payload));
    await listen<{ id: string; data: string }>("term-output", (e) => outputs.get(e.payload.id)?.(e.payload.data));
    await listen<{ id: string }>("term-exit", (e) => exits.get(e.payload.id)?.());
    await listen<{ id: string; cols: number; rows: number }>("term-size", (e) => ptySize.set(e.payload.id, `${e.payload.cols}x${e.payload.rows}`));
    this.sync(await invoke<TermInfo[]>("term_list"));
  }

  private sync(list: TermInfo[]) {
    this.list = list;
    if (!list.some((t) => t.id === this.active)) this.active = list.at(-1)?.id ?? null;
  }

  setOpen(open: boolean) {
    this.open = open;
    save("hrns.termOpen", open ? "1" : "0");
  }

  setHeight(h: number) {
    this.height = Math.round(Math.min(Math.max(140, h), window.innerHeight - 200));
    save("hrns.termHeight", String(this.height));
  }

  /** Open the panel; start a shell in `cwd` when there's none yet (or when `fresh`). */
  async show(cwd: string | null | undefined, fresh = false) {
    this.setOpen(true);
    if (fresh || !this.list.length) await this.create(cwd);
  }

  async create(cwd: string | null | undefined) {
    const t = await invoke<TermInfo>("term_open", { cwd: cwd ?? null, cols: 100, rows: 24 });
    if (!this.list.some((x) => x.id === t.id)) this.list = [...this.list, t];
    this.active = t.id;
  }

  async close(id: string) {
    const i = this.list.findIndex((t) => t.id === id);
    this.list = this.list.filter((t) => t.id !== id);
    if (this.active === id) this.active = this.list[Math.min(i, this.list.length - 1)]?.id ?? null;
    if (!this.list.length) this.setOpen(false);
    await invoke("term_close", { id });
  }

  /** Route a session's output and exit to its view. */
  attach(id: string, onData: Output, onExit: () => void) {
    outputs.set(id, onData);
    exits.set(id, onExit);
    return () => {
      if (outputs.get(id) === onData) outputs.delete(id);
      if (exits.get(id) === onExit) exits.delete(id);
    };
  }
}

export const terms = new Terms();
