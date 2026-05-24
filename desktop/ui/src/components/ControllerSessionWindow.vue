<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { MonitorPlay, PlugZap } from "lucide-vue-next";
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
  <section class="grid min-h-screen grid-rows-[56px_minmax(0,1fr)] overflow-hidden bg-slate-950 text-white">
    <header class="flex items-center justify-between border-b border-slate-700 px-5">
      <div class="min-w-0">
        <div class="truncate text-sm font-medium">{{ session.peerName }}</div>
        <div class="mt-1 truncate text-xs text-slate-400">{{ session.peerSubtitle }}</div>
      </div>
      <div class="flex items-center gap-3">
        <div class="inline-flex items-center gap-2 text-xs text-slate-300">
          <PlugZap class="size-3.5" />
          <span>{{ elapsedLabel }}</span>
        </div>
        <Button variant="outline" class="h-9 border-slate-600 bg-transparent px-4 text-white hover:bg-slate-800 hover:text-white" @click="emit('disconnect')">
          断开连接
        </Button>
      </div>
    </header>

    <div class="relative overflow-hidden">
      <div class="absolute inset-0 bg-[linear-gradient(180deg,rgba(11,23,40,0.94),rgba(2,6,23,1))]"></div>
      <div class="absolute inset-0 bg-[linear-gradient(rgba(148,163,184,0.08)_1px,transparent_1px),linear-gradient(90deg,rgba(148,163,184,0.08)_1px,transparent_1px)] bg-[size:32px_32px] opacity-25"></div>
      <div class="relative flex h-full min-h-[640px] items-center justify-center px-6 py-12">
        <div class="max-w-2xl space-y-6 text-center">
          <div class="mx-auto grid size-18 place-items-center border border-slate-700 bg-slate-900/70">
            <MonitorPlay class="size-8" />
          </div>
          <div class="space-y-3">
            <p class="text-xs uppercase tracking-[0.28em] text-white/55">被控端窗口区域</p>
            <h3 class="text-4xl font-semibold tracking-tight md:text-6xl">{{ session.peerName }}</h3>
            <p class="mx-auto max-w-xl text-base leading-8 text-white/62">
              连接建立后，主窗口完整让位给被控画面。后续这里直接接视频渲染、缩放和输入注入层。
            </p>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
