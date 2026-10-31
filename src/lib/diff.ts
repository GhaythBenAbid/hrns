/** Codex sends unified diffs for edits, but the raw file contents for adds/deletes. */
export function diffLines(c: { kind: string; diff: string }): string[] {
  const lines = c.diff.replace(/\n$/, "").split("\n");
  if (c.kind === "add") return lines.map((l) => "+" + l);
  if (c.kind === "delete") return lines.map((l) => "-" + l);
  return lines;
}

export function stat(lines: string[]) {
  let add = 0, del = 0;
  for (const l of lines) {
    if (l.startsWith("+") && !l.startsWith("+++")) add++;
    else if (l.startsWith("-") && !l.startsWith("---")) del++;
  }
  return { add, del };
}
