import { CameraView, useCameraPermissions } from "expo-camera";
import { useRef, useState } from "react";
import { ActivityIndicator, KeyboardAvoidingView, Platform, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { Client, parsePairing, type Server } from "../api";
import { c } from "../theme";

export function Pair({ onPaired }: { onPaired: (s: Server) => void }) {
  const [permission, requestPermission] = useCameraPermissions();
  const [scanning, setScanning] = useState(false);
  const [manual, setManual] = useState(false);
  const [url, setUrl] = useState("");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const handled = useRef(false);

  async function tryPair(server: Server) {
    setBusy(true);
    setError(null);
    try {
      await new Client(server).info();
      onPaired(server);
    } catch (e) {
      setError((e as Error).message);
      handled.current = false;
    } finally {
      setBusy(false);
    }
  }

  async function startScan() {
    if (!permission?.granted) {
      const p = await requestPermission();
      if (!p.granted) return setError("Camera access is needed to scan the pairing code.");
    }
    handled.current = false;
    setScanning(true);
  }

  if (scanning) {
    return (
      <View style={styles.scanWrap}>
        <CameraView
          style={StyleSheet.absoluteFill}
          barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
          onBarcodeScanned={({ data }) => {
            if (handled.current) return;
            const server = parsePairing(data);
            if (!server) return;
            handled.current = true;
            setScanning(false);
            tryPair(server);
          }}
        />
        <View style={styles.frame} pointerEvents="none" />
        <Text style={styles.scanHint}>Point at the QR code in hrns → Settings → Remote</Text>
        <Pressable style={styles.cancel} onPress={() => setScanning(false)}>
          <Text style={styles.cancelText}>Cancel</Text>
        </Pressable>
      </View>
    );
  }

  return (
    <KeyboardAvoidingView style={styles.wrap} behavior={Platform.OS === "ios" ? "padding" : undefined}>
      <View style={styles.mark}>
        <Text style={styles.markText}>{">_"}</Text>
      </View>
      <Text style={styles.title}>Connect to hrns</Text>
      <Text style={styles.lead}>Control your Codex chats from your phone. Your phone and Mac must both be on Tailscale.</Text>

      <View style={styles.steps}>
        {["On your Mac, open hrns → Settings → Remote and turn it on.", "Tap the button below and scan the QR code.", "That's it — your chats appear here, live."].map((s, i) => (
          <View key={i} style={styles.step}>
            <Text style={styles.stepNum}>{i + 1}</Text>
            <Text style={styles.stepText}>{s}</Text>
          </View>
        ))}
      </View>

      {busy ? (
        <ActivityIndicator color={c.accent} style={{ marginTop: 24 }} />
      ) : (
        <Pressable style={styles.primary} onPress={startScan}>
          <Text style={styles.primaryText}>Scan pairing code</Text>
        </Pressable>
      )}

      {manual ? (
        <View style={styles.manual}>
          <TextInput style={styles.input} placeholder="http://100.x.y.z:7788" placeholderTextColor={c.faint} autoCapitalize="none" autoCorrect={false} value={url} onChangeText={setUrl} />
          <TextInput style={styles.input} placeholder="Token" placeholderTextColor={c.faint} autoCapitalize="none" autoCorrect={false} secureTextEntry value={token} onChangeText={setToken} />
          <Pressable style={styles.secondary} onPress={() => tryPair({ url: url.trim().replace(/\/$/, ""), token: token.trim() })}>
            <Text style={styles.secondaryText}>Connect</Text>
          </Pressable>
        </View>
      ) : (
        <Pressable onPress={() => setManual(true)}>
          <Text style={styles.link}>Enter address manually</Text>
        </Pressable>
      )}

      {error && <Text style={styles.error}>{error}</Text>}
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  wrap: { flex: 1, padding: 24, justifyContent: "center" },
  mark: { width: 56, height: 56, borderRadius: 16, backgroundColor: c.raised, borderWidth: 1, borderColor: c.border, alignItems: "center", justifyContent: "center", alignSelf: "center", marginBottom: 18 },
  markText: { color: c.text, fontFamily: c.mono, fontSize: 18, fontWeight: "700" },
  title: { color: c.text, fontSize: 26, fontWeight: "700", textAlign: "center" },
  lead: { color: c.muted, fontSize: 14.5, textAlign: "center", marginTop: 8, lineHeight: 21 },
  steps: { marginTop: 28, gap: 12, backgroundColor: c.panel, borderRadius: 16, borderWidth: 1, borderColor: c.border, padding: 16 },
  step: { flexDirection: "row", gap: 12, alignItems: "flex-start" },
  stepNum: { width: 22, height: 22, borderRadius: 11, backgroundColor: c.raised, color: c.accent, textAlign: "center", lineHeight: 22, fontSize: 12, fontWeight: "700", overflow: "hidden" },
  stepText: { flex: 1, color: c.text, fontSize: 14, lineHeight: 21 },
  primary: { marginTop: 24, height: 50, borderRadius: 14, backgroundColor: c.text, alignItems: "center", justifyContent: "center" },
  primaryText: { color: c.bg, fontSize: 16, fontWeight: "700" },
  link: { color: c.muted, textAlign: "center", marginTop: 16, fontSize: 14 },
  manual: { marginTop: 18, gap: 10 },
  input: { height: 46, borderRadius: 12, borderWidth: 1, borderColor: c.border, backgroundColor: c.panel, color: c.text, paddingHorizontal: 14, fontSize: 14 },
  secondary: { height: 46, borderRadius: 12, borderWidth: 1, borderColor: c.border, alignItems: "center", justifyContent: "center" },
  secondaryText: { color: c.text, fontSize: 15, fontWeight: "600" },
  error: { color: c.danger, textAlign: "center", marginTop: 16, fontSize: 13.5 },
  scanWrap: { flex: 1, backgroundColor: "#000", alignItems: "center", justifyContent: "center" },
  frame: { width: 240, height: 240, borderRadius: 24, borderWidth: 3, borderColor: c.accent },
  scanHint: { position: "absolute", top: 80, left: 24, right: 24, color: "#fff", textAlign: "center", fontSize: 15, fontWeight: "600" },
  cancel: { position: "absolute", bottom: 60, paddingHorizontal: 28, paddingVertical: 12, borderRadius: 24, backgroundColor: "rgba(0,0,0,0.6)" },
  cancelText: { color: "#fff", fontSize: 16, fontWeight: "600" },
});
