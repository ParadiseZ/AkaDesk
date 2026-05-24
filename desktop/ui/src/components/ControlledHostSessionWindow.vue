<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { MonitorDot, ShieldOff, Smartphone } from "lucide-vue-next";
import type { ActiveSessionPresentation } from "@/types/shell";
import { Button } from "@/components/ui/button";

const props = defineProps<{
  session: ActiveSessionPresentation;
}>();

const emit = defineEmits<{
  disconnect: [];
}>();

const now = ref(Date.now());
let timer = 0;

onMounted(() => {
  timer = window.setInterval(() => {
    now.value = Date.now();
  }, 1000);
});

onBeforeUnmount(() => {
  window.clearInterval(timer);
});

const elapsedLabel = computed(() => {
  const totalSeconds = Math.max(
    0,
    Math.floor((now.value - props.session.startedAtMs) / 1000),
  );
  const minutes = String(Math.floor(totalSeconds / 60)).padStart(2, "0");
  const seconds = String(totalSeconds % 60).padStart(2, "0");
  return `00:${minutes}:${seconds}`;
});
</script>

<template>
  <section class="grid min-h-screen place-items-center bg-[#eef5ff] px-6 py-8">
    <div class="grid w-full max-w-[920px] border border-slate-300 bg-white">
      <div class="bg-[linear-gradient(180deg,#3b82f6_0%,#2b6fd3_100%)] px-6 py-7 text-white">
        <div class="flex items-center justify-between gap-4">
          <div>
            <h2 class="text-4xl font-semibold tracking-tight">本机被控中 {{ elapsedLabel }}</h2>
            <p class="mt-2 text-sm text-white/78">{{ session.peerSubtitle }}</p>
          </div>
          <Button variant="outline" class="h-10 border-white/30 bg-white text-slate-900 hover:bg-slate-100" @click="emit('disconnect')">
            <ShieldOff class="size-4" />
            断开所有远控
          </Button>
        </div>
      </div>

      <div class="border-t border-slate-300 px-6 py-5">
        <div class="mb-3 text-sm font-medium text-slate-700">本账号设备</div>
        <div class="grid gap-4 border border-slate-300 bg-white p-4 md:grid-cols-[auto_1fr_auto] md:items-center">
          <div class="grid size-12 place-items-center bg-sky-600 text-white">
            <Smartphone class="size-5" />
          </div>
          <div class="space-y-1">
            <div class="flex items-center gap-3">
              <strong class="block text-base">{{ session.peerName }}</strong>
              <span class="inline-flex items-center gap-1 text-xs text-slate-500">
                <MonitorDot class="size-3.5" />
                远程控制中
              </span>
            </div>
            <span class="text-sm text-slate-500">远程控制 | 连接时长 {{ elapsedLabel }}</span>
          </div>
          <Button variant="outline" class="h-10 border-slate-300 px-4" @click="emit('disconnect')">
            断开远控
          </Button>
        </div>
      </div>
    </div>
  </section>
</template>
