import * as Notifications from "expo-notifications";
import { Platform } from "react-native";

// Local notifications driven by the live connection to hrns.
// (True remote push while the app is killed needs a dev build + FCM.)

export const APPROVAL = "approval";

Notifications.setNotificationHandler({
  handleNotification: async () => ({ shouldShowBanner: true, shouldShowList: true, shouldPlaySound: true, shouldSetBadge: false }),
});

let ready: Promise<void> | null = null;

/** Channel + action buttons. Never prompts; permission is asked only from our own UI. */
export function setupNotifications(): Promise<void> {
  ready ??= (async () => {
    // Best-effort: a failure here must never block later notifications.
    if (Platform.OS === "android") {
      await Notifications.setNotificationChannelAsync("codex", {
        name: "Codex activity",
        importance: Notifications.AndroidImportance.HIGH,
      }).catch(() => {});
    }
    await Notifications.setNotificationCategoryAsync(APPROVAL, [
      { identifier: "approve", buttonTitle: "Approve", options: { opensAppToForeground: false } },
      { identifier: "deny", buttonTitle: "Deny", options: { opensAppToForeground: false, isDestructive: true } },
    ]).catch(() => {});
  })();
  return ready;
}

export async function notificationsEnabled(): Promise<boolean> {
  return (await Notifications.getPermissionsAsync().catch(() => ({ granted: false }))).granted;
}

/** Shows the system prompt — call only after the user tapped "Enable". */
export async function enableNotifications(): Promise<boolean> {
  await setupNotifications();
  return (await Notifications.requestPermissionsAsync().catch(() => ({ granted: false }))).granted;
}

export async function notify(title: string, body: string, data: Record<string, string> = {}, category?: string) {
  await setupNotifications();
  if (!(await notificationsEnabled())) return;
  await Notifications.scheduleNotificationAsync({
    content: { title, body, data, sound: true, ...(category ? { categoryIdentifier: category } : {}) },
    trigger: Platform.OS === "android" ? { channelId: "codex" } : null,
  });
}

export const onNotificationResponse = Notifications.addNotificationResponseReceivedListener;
