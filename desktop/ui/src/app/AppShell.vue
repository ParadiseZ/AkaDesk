<script setup lang="ts">
import { computed, onMounted } from "vue";
import { RouterView, useRoute, useRouter } from "vue-router";
import AppSidebar from "@/components/AppSidebar.vue";
import ControlledHostSessionWindow from "@/components/ControlledHostSessionWindow.vue";
import IncomingRequestPopup from "@/components/IncomingRequestPopup.vue";
import { useShellStore } from "@/stores/shell";

const store = useShellStore();
const router = useRouter();
const route = useRoute();

onMounted(async () => {
  await store.bootstrap();
  const savedRoute = store.shellSettings?.lastRoute;
  if (route.path === '/remote' && savedRoute && ['/remote', '/settings'].includes(savedRoute)) {
    await router.replace(savedRoute);
  }
});

const activeSession = computed(() => store.snapshot?.activeSession ?? null);
const activeRole = computed(() => activeSession.value?.role ?? null);
</script>

<template>
  <div v-if="store.errorMessage" role="alert" class="fixed left-1/2 top-4 z-[100] flex max-w-[90vw] -translate-x-1/2 items-center gap-4 border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-800 shadow">
    <span>{{ store.errorMessage }}</span>
    <button type="button" class="shrink-0 underline" @click="store.clearError">关闭</button>
  </div>
  <ControlledHostSessionWindow
    v-if="store.snapshot?.desktopUiState === 'SESSION_ACTIVE_VIEW' && activeRole === 'controlled_host' && activeSession"
    :session="activeSession"
    :streaming="store.snapshot?.runtimeStatusLabel === 'STREAMING'"
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
    :key="store.snapshot.incomingRequest.requestId"
    @allow="store.respondIncomingRequest($event, true)"
    @reject="store.respondIncomingRequest($event, false)"
  />
</template>
