import { invoke } from "@tauri-apps/api/core";

export type GitFile = { path: string; old: string | null; kind: string; add: number; del: number; binary: boolean; untracked: boolean };
export type GitStatus = { root: string; rev: string; files: GitFile[]; add: number; del: number };

class GitState {
  /** Uncommitted changes per project folder (null = not a git repo). Raw: replaced whole, never deep-proxied. */
  status = $state.raw<Record<string, GitStatus | null>>({});
}
export const git = new GitState();

const inflight = new Set<string>();

export async function refreshDiff(dir: string | null | undefined) {
  if (!dir || inflight.has(dir)) return;
  inflight.add(dir);
  try {
    const next = await invoke<GitStatus | null>("git_status", { path: dir }).catch(() => null);
    const prev = git.status[dir];
    // Same fingerprint: nothing changed on disk, so leave the UI alone.
    if (dir in git.status && (prev?.rev ?? null) === (next?.rev ?? null)) return;
    git.status = { ...git.status, [dir]: next };
  } finally {
    inflight.delete(dir);
  }
}

/** Refresh every folder on screen on window focus and on a slow timer while visible. */
export function watchDiffs(dirs: () => (string | null | undefined)[]) {
  const all = () => new Set(dirs()).forEach(refreshDiff);
  const timer = setInterval(() => document.visibilityState === "visible" && document.hasFocus() && all(), 15_000);
  window.addEventListener("focus", all);
  return () => {
    clearInterval(timer);
    window.removeEventListener("focus", all);
  };
}

// One file's patch, fetched on demand; dropped when the repo's fingerprint moves on.
const patches = new Map<string, { rev: string; files: Map<string, Promise<string | null>> }>();

export function fileDiff(s: GitStatus, f: GitFile): Promise<string | null> {
  let repo = patches.get(s.root);
  if (!repo || repo.rev !== s.rev) patches.set(s.root, (repo = { rev: s.rev, files: new Map() }));
  let p = repo.files.get(f.path);
  if (!p) {
    p = invoke<string | null>("git_file_diff", { root: s.root, path: f.path, old: f.old, untracked: f.untracked }).catch(() => null);
    repo.files.set(f.path, p);
  }
  return p;
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** A patch as one HTML string: one parse instead of thousands of component nodes. */
export function diffHtml(patch: string): string {
  let o = 1, n = 1;
  let out = "";
  for (const l of patch.split("\n")) {
    const h = l.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)/);
    if (h) {
      o = +h[1];
      n = +h[2];
      out += `<div class="r h"><i></i><i></i><b></b><span>${esc(h[3].trim() || l)}</span></div>`;
    } else if (l.startsWith("\\")) {
      continue; // "\ No newline at end of file"
    } else if (l.startsWith("+")) {
      out += `<div class="r a" data-n="${n}"><i></i><i>${n++}</i><b>+</b><span>${esc(l.slice(1))}</span></div>`;
    } else if (l.startsWith("-")) {
      out += `<div class="r d" data-o="${o}"><i>${o++}</i><i></i><b>−</b><span>${esc(l.slice(1))}</span></div>`;
    } else {
      out += `<div class="r" data-o="${o}" data-n="${n}"><i>${o++}</i><i>${n++}</i><b></b><span>${esc(l.slice(1))}</span></div>`;
    }
  }
  return out;
}

/** A review comment on one line of the uncommitted diff, to send to the agent. */
export type ReviewComment = { id: string; path: string; line: number; side: "new" | "old"; code: string; text: string };

class Reviews {
  /** Draft comments per repository root. */
  byRoot = $state<Record<string, ReviewComment[]>>({});

  of(root: string | null | undefined) {
    return (root && this.byRoot[root]) || [];
  }
  add(root: string, c: Omit<ReviewComment, "id">) {
    this.byRoot[root] = [...this.of(root), { ...c, id: crypto.randomUUID() }];
  }
  remove(root: string, id: string) {
    this.byRoot[root] = this.of(root).filter((c) => c.id !== id);
  }
  clear(root: string) {
    delete this.byRoot[root];
  }
}
export const reviews = new Reviews();

/** The comments as one message for the agent, in file and line order. */
export function reviewMessage(comments: ReviewComment[]): string {
  const sorted = [...comments].sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line);
  const items = sorted.map((c, i) => {
    const where = c.side === "old" ? `${c.path}, removed line ${c.line}` : `${c.path}:${c.line}`;
    const code = c.code.trim() ? `\n   \`${c.code.trim().slice(0, 200)}\`` : "";
    return `${i + 1}. ${where}${code}\n   ${c.text.trim().replace(/\n/g, "\n   ")}`;
  });
  return `I reviewed the uncommitted changes. Please address these comments:\n\n${items.join("\n\n")}`;
}
