import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { desktopApi } from "@/services/desktopApi";
import type {
  HostSettingsInput,
  RemoteConnectRequest,
  ShellActionReceipt,
  ShellSettings,
  ShellSnapshot,
} from "@/types/shell";

export const useShellStore = defineStore("shell", () => {
  const snapshot = ref<ShellSnapshot | null>(null);
  const loading = ref(false);
  const actionMessage = ref("");
  const lastReceipt = ref<ShellActionReceipt | null>(null);
  let unlisten: (() => void) | null = null;

  async function bootstrap() {
    loading.value = true;
    try {
      snapshot.value = await desktopApi.bootstrapShell();
      if (!unlisten) {
        unlisten = await desktopApi.onShellState((next) => {
          snapshot.value = next;
        });
      }
    } finally {
      loading.value = false;
    }
  }

  async function refresh() {
    snapshot.value = await desktopApi.refreshShell();
  }

  async function saveShellSettings(settings: ShellSettings) {
    snapshot.value = await desktopApi.saveShellSettings(settings);
  }

  async function updateHostSettings(settings: HostSettingsInput) {
    snapshot.value = await desktopApi.updateHostSettings(settings);
  }

  async function refreshTemporaryVerificationCode() {
    snapshot.value = await desktopApi.refreshTemporaryVerificationCode();
  }

  async function queueRemoteConnect(request: RemoteConnectRequest) {
    lastReceipt.value = await desktopApi.queueRemoteConnect(request);
    actionMessage.value = lastReceipt.value.message;
  }

  async function previewIncomingRequest() {
    snapshot.value = await desktopApi.previewIncomingRequest();
  }

  async function respondIncomingRequest(allow: boolean) {
    snapshot.value = await desktopApi.respondIncomingRequest(allow);
  }

  async function disconnectSession() {
    snapshot.value = await desktopApi.disconnectSession();
    actionMessage.value = "";
    lastReceipt.value = null;
  }

  const hostSettings = computed(() => snapshot.value?.hostOverview.settings ?? null);
  const shellSettings = computed(() => snapshot.value?.shellSettings ?? null);
  const messages = computed(() => snapshot.value?.messages ?? []);

  return {
    snapshot,
    loading,
    actionMessage,
    lastReceipt,
    hostSettings,
    shellSettings,
    messages,
    bootstrap,
    refresh,
    saveShellSettings,
    updateHostSettings,
    refreshTemporaryVerificationCode,
    queueRemoteConnect,
    previewIncomingRequest,
    respondIncomingRequest,
    disconnectSession,
  };
});
