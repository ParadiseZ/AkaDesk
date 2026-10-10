import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { desktopApi } from "@/services/desktopApi";
import type { HostSettingsInput, ShellSettings, ShellSnapshot } from "@/types/shell";

export const useShellStore = defineStore("shell", () => {
  const snapshot = ref<ShellSnapshot | null>(null);
  const loading = ref(false);
  const errorMessage = ref("");
  let unlistenError: (() => void) | null = null;
  let unlisten: (() => void) | null = null;
  let mutations: Promise<unknown> = Promise.resolve();

  function accept(next: ShellSnapshot) {
    if (!snapshot.value || next.updatedAtMs >= snapshot.value.updatedAtMs) snapshot.value = next;
  }
  async function perform(action: () => Promise<ShellSnapshot>): Promise<boolean> {
    try { accept(await action()); errorMessage.value = ""; return true; }
    catch (error) { errorMessage.value = error instanceof Error ? error.message : String(error); return false; }
  }
  function mutate(action: () => Promise<ShellSnapshot>) {
    const result = mutations.then(() => perform(action));
    mutations = result;
    return result;
  }
  async function bootstrap() {
    loading.value = true;
    try {
      // Subscribe first so a change between the initial snapshot and subscription is retained.
      if (!unlisten) unlisten = await desktopApi.onShellState(accept);
      if (!unlistenError) unlistenError = await desktopApi.onShellError(error => { errorMessage.value = error; if (snapshot.value) snapshot.value.ready = false; });
      await perform(() => desktopApi.bootstrapShell());
    } catch (error) { errorMessage.value = String(error); }
    finally { loading.value = false; }
  }
  const refresh = () => perform(() => desktopApi.refreshShell());
  const saveShellSettings = (settings: ShellSettings) => mutate(() => desktopApi.saveShellSettings(settings));
  const updateHostSettings = (patch: Partial<HostSettingsInput>) => mutate(() => {
    if (!snapshot.value?.ready) throw new Error("请先启动 AkaDesk 桌面应用");
    return desktopApi.updateHostSettings({ ...snapshot.value.hostOverview.settings, ...patch });
  });
  const refreshTemporaryVerificationCode = () => mutate(() => desktopApi.refreshTemporaryVerificationCode());
  const respondIncomingRequest = (requestId: string, allow: boolean) => mutate(() => desktopApi.respondIncomingRequest(requestId, allow));
  const disconnectSession = () => mutate(() => desktopApi.disconnectSession());
  const hostSettings = computed(() => snapshot.value?.hostOverview.settings ?? null);
  const shellSettings = computed(() => snapshot.value?.shellSettings ?? null);
  const messages = computed(() => snapshot.value?.messages ?? []);
  function clearError() { errorMessage.value = ""; }
  return { snapshot, loading, errorMessage, clearError, hostSettings, shellSettings, messages,
    bootstrap, refresh, saveShellSettings, updateHostSettings, refreshTemporaryVerificationCode,
    respondIncomingRequest, disconnectSession };
});
