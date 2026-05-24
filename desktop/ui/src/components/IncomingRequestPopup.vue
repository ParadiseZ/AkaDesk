<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { ShieldCheck, ShieldX } from "lucide-vue-next";
import type { IncomingRequestPreview } from "@/types/shell";
import { Button } from "@/components/ui/button";

const props = defineProps<{
  request: IncomingRequestPreview;
}>();

const emit = defineEmits<{
  allow: [];
  reject: [];
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

const countdown = computed(() =>
  Math.max(0, Math.ceil((props.request.expiresAtMs - now.value) / 1000)),
);
</script>

<template>
  <section class="fixed right-6 bottom-6 z-50 w-[360px] animate-in slide-in-from-bottom-3 border border-slate-300 bg-white shadow-lg">
    <div class="border-b border-slate-300 px-5 py-4">
      <div class="text-sm font-medium">收到 {{ request.deviceLabel }} 用户的远控申请</div>
      <div class="mt-1 text-xs text-slate-500">{{ request.controllerName }} 正在等待本机授权。</div>
    </div>
    <div class="grid gap-4 px-5 py-4">
      <div class="flex items-center justify-between border border-slate-300 px-4 py-3 text-sm">
        <span>自动拒绝倒计时</span>
        <strong>{{ countdown }}s</strong>
      </div>
      <div class="grid grid-cols-2 gap-3">
        <Button class="h-10 bg-sky-600 hover:bg-sky-700" @click="emit('allow')">
          <ShieldCheck class="size-4" />
          允许
        </Button>
        <Button variant="outline" class="h-10 border-slate-300" @click="emit('reject')">
          <ShieldX class="size-4" />
          拒绝
        </Button>
      </div>
    </div>
  </section>
</template>
