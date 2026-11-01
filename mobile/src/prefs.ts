import * as SecureStore from "expo-secure-store";
import type { Prefs } from "./api";
import type { Appearance } from "./theme";

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

// Appearance is per phone, kept apart from chat defaults.
const LOOK = "hrns.appearance";

export async function loadAppearance(): Promise<Appearance> {
  try {
    return JSON.parse((await SecureStore.getItemAsync(LOOK)) ?? "{}");
  } catch {
    return {};
  }
}

export async function saveAppearance(a: Appearance) {
  await SecureStore.setItemAsync(LOOK, JSON.stringify(a));
}
