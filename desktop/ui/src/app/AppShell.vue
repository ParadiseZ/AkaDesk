<script setup lang="ts">
import { computed, onMounted } from "vue";
import { RouterView } from "vue-router";
import AppSidebar from "@/components/AppSidebar.vue";
import ControlledHostSessionWindow from "@/components/ControlledHostSessionWindow.vue";
import ControllerSessionWindow from "@/components/ControllerSessionWindow.vue";
import IncomingRequestPopup from "@/components/IncomingRequestPopup.vue";
import { useShellStore } from "@/stores/shell";

const store = useShellStore();

onMounted(() => {
  void store.bootstrap();
});

const activeSession = computed(() => store.snapshot?.activeSession ?? null);
const activeRole = computed(() => activeSession.value?.role ?? null);
</script>

<template>
  <ControllerSessionWindow
    v-if="store.snapshot?.desktopUiState === 'SESSION_ACTIVE_VIEW' && activeRole === 'controller' && activeSession"
    :session="activeSession"
    @disconnect="store.disconnectSession"
  />

  <ControlledHostSessionWindow
    v-else-if="store.snapshot?.desktopUiState === 'SESSION_ACTIVE_VIEW' && activeRole === 'controlled_host' && activeSession"
    :session="activeSession"
    @disconnect="store.disconnectSession"
  />

  <div v-else class="grid h-screen grid-cols-[236px_minmax(0,1fr)] overflow-hidden bg-[#eaf4ff]">
    <div class="overflow-y-auto">
      <AppSidebar />
    </div>
    <main class="min-w-0 overflow-y-auto border-l border-slate-300 bg-white">
      <RouterView />
    </main>
  </div>

  <IncomingRequestPopup
    v-if="store.snapshot?.incomingRequest"
    :request="store.snapshot.incomingRequest"
    @allow="store.respondIncomingRequest(true)"
    @reject="store.respondIncomingRequest(false)"
  />
</template>
