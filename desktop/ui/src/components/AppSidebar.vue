<script setup lang="ts">
import { routerAndIcon } from "@/app/router";
import { computed } from "vue";
import { useRoute } from "vue-router";
import { cn } from "@/lib/utils";

const route = useRoute();

const activePath = computed(() => route.path);
</script>

<template>
  <aside class="flex h-full flex-col bg-white text-slate-900">
    <div class="border-b border-slate-300 px-5 py-5 flex justify-center">
      <h1 class="mt-2 text-2xl font-semibold tracking-tight">AKADESK</h1>
    </div>

    <nav class="grid gap-px bg-slate-300">
      <RouterLink
        v-for="item in routerAndIcon"
        :key="item.path"
        :to="item.path"
        :class="
          cn(
            'flex h-12 items-center gap-3 bg-white px-5 text-sm text-slate-900 transition-colors hover:bg-slate-200',
            activePath === item.path && 'bg-sky-500 font-medium text-white hover:bg-sky-500',
          )
        "
      >
        <component :is="item.icon" class="size-4" />
        <span>{{ item.title }}</span>
      </RouterLink>
    </nav>

    <div class="mt-auto border-t border-slate-300 px-5 py-4 text-xs leading-6 text-slate-500">
      支持 Android 连接本机查看屏幕。
    </div>
  </aside>
</template>
