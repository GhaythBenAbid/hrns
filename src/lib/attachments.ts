import { invoke } from "@tauri-apps/api/core";

export type Attachment = { path: string; name: string; image: boolean };

const IMAGE = /\.(png|jpe?g|gif|webp)$/i;
export const isImage = (path: string) => IMAGE.test(path);
export const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
export const toAttachment = (path: string): Attachment => ({ path, name: fileName(path), image: isImage(path) });

// Previews are read once per path and kept for the session.
const previews = new Map<string, Promise<string | null>>();
export function preview(path: string): Promise<string | null> {
  let p = previews.get(path);
  if (!p) previews.set(path, (p = invoke<string | null>("attachment_preview", { path }).catch(() => null)));
  return p;
}

/** Save a pasted file (e.g. a screenshot from the clipboard) and return it as an attachment. */
export async function savePasted(file: File): Promise<Attachment> {
  const buf = new Uint8Array(await file.arrayBuffer());
  let bin = "";
  for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
  const ext = file.type.split("/")[1]?.replace("jpeg", "jpg") || file.name.split(".").pop() || "bin";
  const path = await invoke<string>("save_attachment", { data: btoa(bin), ext });
  const a = toAttachment(path);
  if (file.type.startsWith("image/")) previews.set(path, Promise.resolve(URL.createObjectURL(file)));
  return { ...a, name: file.name && file.name !== "image.png" ? file.name : a.name };
}
