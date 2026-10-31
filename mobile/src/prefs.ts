import * as SecureStore from "expo-secure-store";
import type { Prefs } from "./api";

// Defaults for chats started from the phone (existing chats keep their own settings).
const KEY = "hrns.prefs";

export async function loadPrefs(): Promise<Prefs> {
  try {
    return JSON.parse((await SecureStore.getItemAsync(KEY)) ?? "{}");
  } catch {
    return {};
  }
}

export async function savePrefs(p: Prefs) {
  await SecureStore.setItemAsync(KEY, JSON.stringify(p));
}
