export interface RuntimeStatusView {
  stateLabel: string;
  activeSessionId: string | null;
  updatedAtMs: number;
}

export interface CapabilityView {
  supportsAudio: boolean;
  supportsInput: boolean;
  supportsDisplayControl: boolean;
  supportsPointer: boolean;
  supportsScroll: boolean;
  supportsKeyboard: boolean;
  updatedAtMs: number;
}

export interface HostIdentityView {
  deviceId: string;
  displayName: string;
  hostname: string;
  appVersion: string;
  platform: string;
}

export type VerificationMode = "TEMPORARY_CODE" | "CUSTOM_CODE" | "BOTH_CODES";
export type ConnectionMethod = "ACCESS_CODE" | "TRUST_ONLY" | "ACCESS_CODE_AND_CONSENT";

export interface HostSettingsView {
  allowRemoteAssistance: boolean;
  verificationMode: VerificationMode;
  connectionMethod: ConnectionMethod;
  notifyOnIncomingRequest: boolean;
  autoAcceptTrustedController: boolean;
  temporaryVerificationCode: string;
  customVerificationCode: string;
  refreshTemporaryCodeAfterControlledEnd: boolean;
  updatedAtMs: number;
}

export interface HostOverviewView {
  identity: HostIdentityView;
  settings: HostSettingsView;
  pairedControllerCount: number;
  controlPort: number;
  activeControllerDisplayName: string | null;
  updatedAtMs: number;
}

export interface ShellSettings {
  launchOnStartup: boolean;
  minimizeToTray: boolean;
  notificationsEnabled: boolean;
  lastRoute: string;
  lastRemoteDeviceId: string;
  lastAccessCode: string;
}

export interface ShellMessage {
  source: string;
  level: string;
  code: string;
  message: string;
  createdAtMs: number;
}

export interface IncomingRequestPreview {
  requestId: string;
  controllerName: string;
  deviceLabel: string;
  expiresAtMs: number;
}

export interface ActiveSessionPresentation {
  sessionId: string;
  role: "controller" | "controlled_host" | string;
  peerName: string;
  peerSubtitle: string;
  startedAtMs: number;
}

export interface ShellSnapshot {
  ready: boolean;
  runtimeStatusLabel: string;
  desktopUiState: string;
  supportsInput: boolean;
  runtimeStatus: RuntimeStatusView;
  capability: CapabilityView;
  hostOverview: HostOverviewView;
  shellSettings: ShellSettings;
  messages: ShellMessage[];
  incomingRequest: IncomingRequestPreview | null;
  activeSession: ActiveSessionPresentation | null;
  updatedAtMs: number;
}

export interface HostSettingsInput {
  allowRemoteAssistance: boolean;
  verificationMode: VerificationMode;
  connectionMethod: ConnectionMethod;
  notifyOnIncomingRequest: boolean;
  autoAcceptTrustedController: boolean;
  temporaryVerificationCode: string;
  customVerificationCode: string;
  refreshTemporaryCodeAfterControlledEnd: boolean;
}

export interface RemoteConnectRequest {
  remoteDeviceId: string;
  accessCode: string;
}

export interface ShellActionReceipt {
  accepted: boolean;
  message: string;
  createdAtMs: number;
}
