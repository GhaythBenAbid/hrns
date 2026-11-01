import { app } from "./store.svelte";

export type Zone = "left" | "right";

/** A chat being dragged out of the sidebar to open it in split view. */
export const drag = $state({ id: null as string | null, title: "", x: 0, y: 0, zone: null as Zone | null });

// Pointer events rather than HTML5 drag and drop: the webview reserves native drops for files.
const THRESHOLD = 6;
let target: HTMLElement | null = null;

export function setDropTarget(el: HTMLElement | null) {
  target = el;
}

function zoneAt(x: number, y: number): Zone | null {
  const r = target?.getBoundingClientRect();
  if (!r || x < r.left || x > r.right || y < r.top || y > r.bottom) return null;
  // With one pane, dropping the chat that's already open would split it with itself.
  if (app.panes.length === 1 && app.view === "chat" && app.current?.id === drag.id) return null;
  return x < r.left + r.width / 2 ? "left" : "right";
}

export function startDrag(e: PointerEvent, id: string, title: string) {
  if (e.button !== 0) return;
  const sx = e.clientX, sy = e.clientY;
  let active = false;

  function move(ev: PointerEvent) {
    if (!active) {
      if (Math.hypot(ev.clientX - sx, ev.clientY - sy) < THRESHOLD) return;
      active = true;
      drag.id = id;
      drag.title = title;
      document.body.classList.add("dragging");
    }
    drag.x = ev.clientX;
    drag.y = ev.clientY;
    drag.zone = zoneAt(ev.clientX, ev.clientY);
  }

  function end(ev: PointerEvent) {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", end);
    window.removeEventListener("pointercancel", end);
    window.removeEventListener("keydown", esc, true);
    if (!active) return;
    const zone = ev.type === "pointerup" ? drag.zone : null;
    cancel();
    // The release would otherwise click whatever is under the pointer (or the card it started on).
    const swallow = (c: MouseEvent) => { c.stopPropagation(); c.preventDefault(); };
    window.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallow, true), 0);
    if (zone) app.openSplit(id, zone);
  }

  function esc(ev: KeyboardEvent) {
    if (ev.key !== "Escape" || !active) return;
    ev.stopPropagation();
    end(new PointerEvent("pointercancel"));
  }

  function cancel() {
    drag.id = null;
    drag.zone = null;
    document.body.classList.remove("dragging");
  }

  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", end);
  window.addEventListener("pointercancel", end);
  window.addEventListener("keydown", esc, true);
}
