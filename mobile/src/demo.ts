// Demo mode for screenshots (web only): `?demo=chats|chat|usage|settings`.
// Replaces the network with sample data so no real chats or accounts are shown.

import { Platform } from "react-native";
import type { Server } from "./api";

export const demoScene: string | null =
  Platform.OS === "web" && typeof window !== "undefined" ? new URLSearchParams(window.location.search).get("demo") : null;

export const DEMO_SERVER: Server = { url: "https://demo.hrns.local", token: "demo" };

const H = 3600_000;

export function installDemo() {
  const now = Date.now();
  const usage = (model: string, i: number, o: number) => ({ [model]: { input: i, cached: Math.round(i * 0.9), output: o, reasoning: Math.round(o * 0.3) } });
  const threads = [
    { id: "t1", title: "Add a dark mode toggle to settings", cwd: "/Users/alex/code/hrns", model: "gpt-6.1-sol", preview: "Added a Theme section to Settings with System / Light / Dark, persisted per device.", messages: 4, files: 3, additions: 86, deletions: 12, commands: 6, failed: false, updatedAt: now - 3 * 60_000, usage: usage("gpt-6.1-sol", 412_000, 9_800) },
    { id: "t2", title: "Upgrade to React 19", cwd: "/Users/alex/code/hrns", model: "gpt-6.1-sol", preview: "", messages: 3, files: 0, additions: 0, deletions: 0, commands: 2, failed: false, updatedAt: now - 60_000, usage: usage("gpt-6.1-sol", 90_000, 1_200) },
    { id: "t3", title: "Write tests for the usage parser", cwd: "/Users/alex/code/hrns", model: "gpt-6-astra", preview: "Added cases for re-emitted counters and fork copies; all 3 tests pass.", messages: 2, files: 1, additions: 64, deletions: 0, commands: 4, failed: false, updatedAt: now - 5 * H, usage: usage("gpt-6-astra", 190_000, 7_200) },
    { id: "t4", title: "Rate limit middleware", cwd: "/Users/alex/code/api-server", model: "gpt-6.1-terra", preview: "Token bucket per API key backed by Redis, with a 429 + Retry-After response.", messages: 8, files: 5, additions: 212, deletions: 40, commands: 9, failed: false, updatedAt: now - 26 * H, usage: usage("gpt-6.1-terra", 620_000, 14_000) },
    { id: "t5", title: "Fix flaky checkout e2e test", cwd: "/Users/alex/code/api-server", model: "gpt-6.1-sol", preview: "The test raced the payment webhook; now it waits for the order event.", messages: 4, files: 2, additions: 18, deletions: 7, commands: 5, failed: false, updatedAt: now - 3 * 24 * H, usage: usage("gpt-6.1-sol", 150_000, 4_000) },
  ];

  const chatParts = [
    { type: "reasoning", id: "r1", text: "Find where settings sections are defined and how preferences are persisted." },
    { type: "command", id: "c1", command: "rg -n textSize src", actions: [{ type: "search", command: "rg -n textSize src", query: "textSize", path: "/Users/alex/code/hrns/src" }], status: "completed", exitCode: 0 },
    { type: "command", id: "c2", command: "cat src/lib/Settings.svelte", actions: [{ type: "read", command: "cat", name: "Settings.svelte", path: "/x/Settings.svelte" }], status: "completed", exitCode: 0 },
    { type: "command", id: "c3", command: "cat src/app.css", actions: [{ type: "read", command: "cat", name: "app.css", path: "/x/app.css" }], status: "completed", exitCode: 0 },
    { type: "fileChange", id: "f1", status: "completed", changes: [
      { path: "/x/src/lib/Settings.svelte", kind: "update", diff: "@@\n+a\n+b\n+c\n+d\n+e\n+f\n-g" },
      { path: "/x/src/app.css", kind: "update", diff: "@@\n+a\n+b\n+c\n+d" },
    ] },
    { type: "text", id: "x2", text: "Added a **Theme** setting with three options:\n\n- **System** follows your OS (default)\n- **Light** and **Dark** override it\n\nThe choice is stored per device and applied before first paint, so there's no flash:\n\n```ts\ndocument.documentElement.dataset.theme = app.theme;\n```\n\nType-check passes." },
  ];
  const thread = {
    id: "t1", title: threads[0].title, model: "gpt-6.1-sol", effort: "medium", access: "auto", cwd: "/Users/alex/code/hrns", updatedAt: now,
    messages: [
      { id: "m1", role: "user", text: "Add a dark mode toggle to the settings page. It should follow the system by default.", createdAt: now - 600_000 },
      { id: "m2", role: "assistant", model: "gpt-6.1-sol", text: "", parts: chatParts, createdAt: now - 180_000 },
    ],
  };

  const rows: object[] = [];
  for (let d = 0; d < 30; d++) {
    const hour = new Date(now - (29 - d) * 24 * H).toISOString().slice(0, 13);
    const k = Math.max(0.08, 0.5 + 0.5 * Math.sin(d / 3.2) + (d > 20 ? (d - 20) * 0.18 : 0)) * (d % 7 >= 5 ? 0.35 : 1);
    rows.push({ hour, source: "codex", model: "gpt-6.1-sol", input: 9e6 * k, cached: 8.4e6 * k, output: 42e3 * k, reasoning: 15e3 * k, requests: Math.round(90 * k) });
    if (d > 12) rows.push({ hour, source: "default", model: "gpt-6.1-sol", input: 3.2e6 * k, cached: 2.9e6 * k, output: 16e3 * k, reasoning: 6e3 * k, requests: Math.round(30 * k) });
    if (d > 18) rows.push({ hour, source: "work", model: "gpt-6.1-terra", input: 2.4e6 * k, cached: 2.1e6 * k, output: 12e3 * k, reasoning: 3e3 * k, requests: Math.round(25 * k) });
  }

  const status = (email: string, plan: string, p: number, s: number) => ({
    account: { email, planType: plan },
    rateLimits: { primary: { usedPercent: p, windowDurationMins: 300, resetsAt: now / 1000 + 9000 }, secondary: { usedPercent: s, windowDurationMins: 10080, resetsAt: now / 1000 + 300000 } },
  });
  const models = [
    { id: "gpt-6.1-sol", displayName: "GPT-6.1-Sol", description: "Latest workhorse model for coding and everyday work.", isDefault: true, defaultReasoningEffort: "medium", supportedReasoningEfforts: ["low", "medium", "high", "xhigh"].map((e) => ({ reasoningEffort: e, description: e })) },
    { id: "gpt-6-astra", displayName: "GPT-6-Astra", description: "Frontier intelligence for the most demanding work.", isDefault: false, defaultReasoningEffort: "medium", supportedReasoningEfforts: [] },
    { id: "gpt-6.1-terra", displayName: "GPT-6.1-Terra", description: "Fast and efficient for everyday tasks.", isDefault: false, defaultReasoningEffort: "low", supportedReasoningEfforts: [] },
  ];

  const routes: [RegExp, () => unknown][] = [
    [/\/api\/info$/, () => ({ name: "hrns", host: "MacBook Pro", version: "0.1.0", accounts: [{ id: "default", email: "alex@example.com", plan: "pro" }, { id: "work", email: "alex@acme.dev", plan: "plus" }] })],
    [/\/api\/threads$/, () => threads],
    [/\/api\/threads\/t1$/, () => ({ thread, live: null })],
    [/\/api\/threads\/t2$/, () => ({ thread: { ...thread, id: "t2", title: threads[1].title, messages: [{ id: "u", role: "user", text: "Upgrade the project to React 19 and fix anything that breaks.", createdAt: now }] }, live: { parts: [{ type: "command", id: "k1", command: "npm install react@19 react-dom@19", status: "inProgress" }], approvals: [{ key: "a1", kind: "command", command: "npm install react@19 react-dom@19", reason: "Needs network access to download packages." }] } })],
    [/\/api\/models/, () => models],
    [/\/api\/accounts\/work\/status$/, () => status("alex@acme.dev", "plus", 64, 31)],
    [/\/api\/accounts\/.*\/status$/, () => status("alex@example.com", "pro", 18, 42)],
    [/\/api\/usage/, () => rows],
    [/\/api\/pricing$/, () => ({ "gpt-6.1-sol": { input: 2, output: 10, cached: 0.1 }, "gpt-6-astra": { input: 10, output: 50, cached: 1 }, "gpt-6.1-terra": { input: 1, output: 5, cached: 0.05 } })],
  ];
  const realFetch = globalThis.fetch;
  globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (!url.startsWith(DEMO_SERVER.url)) return realFetch(input, init);
    const hit = routes.find(([re]) => re.test(url.split("?")[0]));
    return new Response(JSON.stringify(hit ? hit[1]() : {}), { status: 200, headers: { "Content-Type": "application/json" } });
  }) as typeof fetch;

  // A WebSocket that just reports "connected".
  class DemoSocket {
    onopen: (() => void) | null = null;
    onmessage: ((m: { data: string }) => void) | null = null;
    onclose: (() => void) | null = null;
    onerror: (() => void) | null = null;
    constructor() {
      setTimeout(() => this.onopen?.(), 10);
    }
    send() {}
    close() {}
  }
  (globalThis as unknown as { WebSocket: unknown }).WebSocket = DemoSocket;
}
