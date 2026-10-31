import type { CommandAction, Part } from "./store.svelte";

type CommandPart = Extract<Part, { type: "command" }>;

/** `/bin/zsh -lc "rtk proxy ls -la"` -> `ls -la` */
export function cleanCommand(cmd: string): string {
  let c = cmd.trim();
  const shell = c.match(/^(?:\/\S*\/)?(?:ba|z)?sh\s+-l?c\s+([\s\S]+)$/);
  if (shell) {
    c = shell[1].trim();
    const q = c[0];
    if ((q === '"' || q === "'") && c.endsWith(q)) {
      c = c.slice(1, -1);
      c = q === '"' ? c.replace(/\\(["\\$`])/g, "$1") : c.replace(/'"'"'/g, "'");
    }
  }
  // rtk is a token-saving wrapper; hide it everywhere in the pipeline.
  return c.replace(/(^|[|;&]\s*)rtk(?:\s+proxy)?\s+/g, "$1").trim();
}

export function relPath(path: string | null | undefined, cwd: string | null | undefined): string {
  if (!path) return ".";
  if (cwd && path.startsWith(cwd)) return path.slice(cwd.length).replace(/^\//, "") || ".";
  const home = path.match(/^\/Users\/[^/]+/)?.[0];
  return home ? "~" + path.slice(home.length) : path;
}

export type Kind = "read" | "list" | "search" | "run";

export type Row = { kind: Kind; label: string; detail?: string };

/** Friendly description of a command, using Codex's own parse when available. */
export function describe(part: CommandPart): Row[] {
  const actions = (part.actions ?? []).filter(Boolean) as CommandAction[];
  const known = actions.length && actions.every((a) => a.type !== "unknown");
  if (!known) return [{ kind: "run", label: cleanCommand(part.command) }];
  return actions.map((a) => {
    switch (a.type) {
      case "read":
        return { kind: "read", label: a.name || relPath(a.path, part.cwd), detail: relPath(a.path, part.cwd) };
      case "listFiles":
        return { kind: "list", label: relPath(a.path, part.cwd) };
      case "search":
        return { kind: "search", label: a.query ? `“${a.query}”` : cleanCommand(a.command), detail: a.path ? `in ${relPath(a.path, part.cwd)}` : undefined };
      default:
        return { kind: "run", label: cleanCommand(a.command) };
    }
  });
}

/** "Read 3 files, 2 searches, ran 1 command" */
export function summarize(parts: Part[]): { verb: string; rest: string } {
  const n = { read: 0, list: 0, search: 0, run: 0, tool: 0 };
  for (const p of parts) {
    if (p.type === "command") for (const r of describe(p)) n[r.kind]++;
    else if (p.type === "tool") n.tool++;
  }
  const bits: string[] = [];
  const plural = (k: number, one: string, many = one + "s") => `${k} ${k === 1 ? one : many}`;
  if (n.read) bits.push(plural(n.read, "file"));
  if (n.list) bits.push(plural(n.list, "folder"));
  if (n.search) bits.push(plural(n.search, "search", "searches"));
  if (n.run) bits.push(plural(n.run, "command"));
  if (n.tool) bits.push(plural(n.tool, "tool call"));
  const verb = n.run || n.tool ? "Worked" : n.read || n.list || n.search ? "Explored" : "Thought";
  return { verb, rest: bits.join(", ") };
}
