import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  HostSettingsInput,
  RemoteConnectRequest,
  ShellActionReceipt,
  ShellSettings,
  ShellSnapshot,
} from "@/types/shell";

const isTauriRuntime =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let mockSnapshot: ShellSnapshot = {
  ready: true,
  runtimeStatusLabel: "READY_FOR_M4",
  desktopUiState: "READY",
  supportsInput: true,
  runtimeStatus: {
    stateLabel: "READY_FOR_M4",
    activeSessionId: null,
    updatedAtMs: Date.now(),
  },
  capability: {
    supportsAudio: true,
    supportsInput: true,
    supportsDisplayControl: true,
    supportsPointer: true,
    supportsScroll: true,
    supportsKeyboard: true,
    updatedAtMs: Date.now(),
  },
  hostOverview: {
    identity: {
      deviceId: "646232332",
      displayName: "AkaDesk Host",
      hostname: "akadesk-dev",
      appVersion: "0.1.0-m4",
      platform: "windows",
    },
    settings: {
      allowRemoteAssistance: true,
      verificationMode: "TEMPORARY_CODE",
      connectionMethod: "ACCESS_CODE",
      notifyOnIncomingRequest: true,
      autoAcceptTrustedController: false,
      temporaryVerificationCode: "646232",
      customVerificationCode: "AKADESK",
      refreshTemporaryCodeAfterControlledEnd: true,
      updatedAtMs: Date.now(),
    },
    pairedControllerCount: 2,
    controlPort: 46661,
    activeControllerDisplayName: null,
    updatedAtMs: Date.now(),
  },
  shellSettings: {
    launchOnStartup: false,
    minimizeToTray: true,
    notificationsEnabled: true,
    lastRoute: "/remote",
    lastRemoteDeviceId: "",
    lastAccessCode: "",
  },
  messages: [
    {
      source: "host-runtime",
      level: "info",
      code: "HOST_RUNTIME_READY",
      message: "Host runtime ready on control port 46661.",
      createdAtMs: Date.now(),
    },
  ],
  incomingRequest: null,
  activeSession: null,
  updatedAtMs: Date.now(),
};

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function pushMockMessage(level: string, code: string, message: string) {
  mockSnapshot.messages.push({
    source: "shell",
    level,
    code,
    message,
    createdAtMs: Date.now(),
  });
  mockSnapshot.messages = mockSnapshot.messages.slice(-20);
  mockSnapshot.updatedAtMs = Date.now();
}

export const desktopApi = {
  async bootstrapShell(): Promise<ShellSnapshot> {
    if (isTauriRuntime) {
      return invoke<ShellSnapshot>("bootstrap_shell");
    }
    return clone(mockSnapshot);
  },

  async refreshShell(): Promise<ShellSnapshot> {
    if (isTauriRuntime) {
      return invoke<ShellSnapshot>("refresh_shell");
    }
    mockSnapshot.updatedAtMs = Date.now();
    return clone(mockSnapshot);
  },

  async saveShellSettings(settings: ShellSettings): Promise<ShellSnapshot> {
    if (isTauriRuntime) {
      return invoke<ShellSnapshot>("save_shell_settings", { settings });
    }
    mockSnapshot.shellSettings = clone(settings);
    pushMockMessage("info", "SHELL_SETTINGS_SAVED", "Desktop shell settings saved.");
    return clone(mockSnapshot);
  },

  async updateHostSettings(settings: HostSettingsInput): Promise<ShellSnapshot> {
    if (isTauriRuntime) {
      return invoke<ShellSnapshot>("update_host_settings", { settings });
    }
    mockSnapshot.hostOverview.settings = {
      ...clone(settings),
      updatedAtMs: Date.now(),
    };
    pushMockMessage(
      "info",
      "HOST_SETTINGS_UPDATED",
      "Remote assistance settings updated in browser preview.",
    );
    return clone(mockSnapshot);
  },

  async refreshTemporaryVerificationCode(): Promise<ShellSnapshot> {
    if (isTauriRuntime) {
      return invoke<ShellSnapshot>("refresh_temporary_verification_code");
    }
    const suffix = String(Date.now() % 1_000_000).padStart(6, "0");
    mockSnapshot.hostOverview.settings.temporaryVerificationCode = suffix;
    mockSnapshot.hostOverview.settings.updatedAtMs = Date.now();
    pushMockMessage(
      "info",
      "HOST_TEMP_CODE_REFRESHED",
      "Temporary verification code refreshed in browser preview.",
    );
    return clone(mockSnapshot);
  },

  async queueRemoteConnect(
    request: RemoteConnectRequest,
  ): Promise<ShellActionReceipt> {
    if (isTauriRuntime) {
      return invoke<ShellActionReceipt>("queue_remote_connect", { request });
    }
    mockSnapshot.shellSettings.lastRemoteDeviceId = request.remoteDeviceId;
    mockSnapshot.shellSettings.lastAccessCode = request.accessCode;
    mockSnapshot.activeSession = {
      role: "controller",
      peerName: request.remoteDeviceId || "Remote Device",
      peerSubtitle: request.accessCode
        ? `验证码 ${request.accessCode}`
        : "正在建立远程控制连接",
      startedAtMs: Date.now(),
    };
    mockSnapshot.desktopUiState = "SESSION_ACTIVE_VIEW";
    pushMockMessage(
      "info",
      "REMOTE_CONNECT_QUEUED",
      `Queued remote request for ${request.remoteDeviceId}.`,
    );
    return {
      accepted: true,
      message: "连接请求已记录到浏览器预览。",
      createdAtMs: Date.now(),
    };
  },

  async previewIncomingRequest(): Promise<ShellSnapshot> {
    if (isTauriRuntime) {
      return invoke<ShellSnapshot>("preview_incoming_request");
    }
    mockSnapshot.incomingRequest = {
      controllerName: "Android Controller",
      deviceLabel: "Pixel 04",
      expiresAtMs: Date.now() + 39_000,
    };
    pushMockMessage(
      "warning",
      "REMOTE_ASSIST_REQUEST_PREVIEW",
      "Incoming remote assist preview emitted.",
    );
    return clone(mockSnapshot);
  },

  async respondIncomingRequest(allow: boolean): Promise<ShellSnapshot> {
    if (isTauriRuntime) {
      return invoke<ShellSnapshot>("respond_incoming_request", { allow });
    }
    mockSnapshot.incomingRequest = null;
    if (allow) {
      mockSnapshot.activeSession = {
        role: "controlled_host",
        peerName: "Android Controller",
        peerSubtitle: "1 台设备正在控制此电脑",
        startedAtMs: Date.now(),
      };
      mockSnapshot.desktopUiState = "SESSION_ACTIVE_VIEW";
    }
    pushMockMessage(
      allow ? "info" : "warning",
      allow ? "REMOTE_ASSIST_REQUEST_ALLOWED" : "REMOTE_ASSIST_REQUEST_REJECTED",
      allow ? "Request preview marked as allowed." : "Request preview marked as rejected.",
    );
    return clone(mockSnapshot);
  },

  async disconnectSession(): Promise<ShellSnapshot> {
    if (isTauriRuntime) {
      return invoke<ShellSnapshot>("disconnect_session");
    }
    mockSnapshot.activeSession = null;
    mockSnapshot.desktopUiState = "READY";
    return clone(mockSnapshot);
  },

  async onShellState(listener: (snapshot: ShellSnapshot) => void): Promise<UnlistenFn> {
    if (isTauriRuntime) {
      return listen<ShellSnapshot>("shell://state", (event) => listener(event.payload));
    }
    const timer = window.setInterval(() => {
      mockSnapshot.updatedAtMs = Date.now();
      listener(clone(mockSnapshot));
    }, 2000);
    return () => window.clearInterval(timer);
  },
};
