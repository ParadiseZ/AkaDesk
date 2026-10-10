import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { HostSettingsInput, ShellSettings, ShellSnapshot } from "@/types/shell";

const isTauriRuntime = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const previewSnapshot: ShellSnapshot = {
  ready: false, runtimeStatusLabel: "OFFLINE", desktopUiState: "READY", supportsInput: false,
  runtimeStatus: { stateLabel: "OFFLINE", activeSessionId: null, updatedAtMs: 0 },
  capability: { supportsAudio: false, supportsInput: false, supportsDisplayControl: false,
    supportsPointer: false, supportsScroll: false, supportsKeyboard: false, updatedAtMs: 0 },
  hostOverview: { identity: { deviceId: "", displayName: "", hostname: "", appVersion: "", platform: "windows" },
    settings: { allowRemoteAssistance: false, verificationMode: "TEMPORARY_CODE", connectionMethod: "ACCESS_CODE",
      notifyOnIncomingRequest: true, autoAcceptTrustedController: false, temporaryVerificationCode: "",
      customVerificationCode: "", refreshTemporaryCodeAfterControlledEnd: true, updatedAtMs: 0 },
    pairedControllerCount: 0, controlPort: 46661, activeControllerDisplayName: null, updatedAtMs: 0 },
  shellSettings: { launchOnStartup: false, minimizeToTray: false, notificationsEnabled: false,
    lastRoute: "/remote", lastRemoteDeviceId: "", lastAccessCode: "" },
  messages: [], incomingRequest: null, activeSession: null, updatedAtMs: 0,
};

function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime) return Promise.reject(new Error("请在 AkaDesk 桌面应用中执行此操作"));
  return invoke<T>(name, args);
}

export const desktopApi = {
  async bootstrapShell(): Promise<ShellSnapshot> {
    return isTauriRuntime ? command("bootstrap_shell") : structuredClone(previewSnapshot);
  },
  async refreshShell(): Promise<ShellSnapshot> {
    return isTauriRuntime ? command("refresh_shell") : structuredClone(previewSnapshot);
  },
  saveShellSettings: (settings: ShellSettings) => command<ShellSnapshot>("save_shell_settings", { settings }),
  updateHostSettings: (settings: HostSettingsInput) => command<ShellSnapshot>("update_host_settings", { settings }),
  refreshTemporaryVerificationCode: () => command<ShellSnapshot>("refresh_temporary_verification_code"),
  respondIncomingRequest: (requestId: string, allow: boolean) =>
    command<ShellSnapshot>("respond_incoming_request", { requestId, allow }),
  disconnectSession: () => command<ShellSnapshot>("disconnect_session"),
  async onShellError(listener: (error: string) => void): Promise<UnlistenFn> {
    return isTauriRuntime ? listen<string>("shell://error", event => listener(event.payload)) : () => {};
  },
  async onShellState(listener: (snapshot: ShellSnapshot) => void): Promise<UnlistenFn> {
    return isTauriRuntime ? listen<ShellSnapshot>("shell://state", event => listener(event.payload)) : () => {};
  },
};
