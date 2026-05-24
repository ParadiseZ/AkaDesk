<script setup lang="ts">
import { Activity, BellRing, RefreshCcw } from "lucide-vue-next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

defineProps<{
  desktopUiState: string;
  runtimeStatusLabel: string;
  updatedLabel: string;
}>();

const emit = defineEmits<{
  refresh: [];
  preview: [];
}>();
</script>

<template>
  <header class="flex flex-col gap-5 border-b border-border/70 px-7 py-6 md:flex-row md:items-start md:justify-between">
    <div class="space-y-2">
      <Badge variant="secondary" class="rounded-full px-3 py-1 text-[11px] uppercase tracking-[0.22em]">
        Desktop Workspace
      </Badge>
      <div class="space-y-1">
        <h2 class="text-2xl font-semibold tracking-tight">远程协助与 Host Runtime 管理</h2>
        <p class="text-sm text-muted-foreground">桌面壳、Host 配置与本地消息桥统一收敛在这个工作区。</p>
      </div>
    </div>

    <div class="flex flex-wrap items-center justify-end gap-2">
      <div class="inline-flex items-center gap-2 rounded-full border bg-card/70 px-4 py-2 text-sm shadow-sm">
        <Activity class="size-4 text-primary" />
        <strong>{{ desktopUiState }}</strong>
        <span class="text-muted-foreground">{{ runtimeStatusLabel }}</span>
      </div>
      <div class="inline-flex items-center gap-2 rounded-full border bg-card/70 px-4 py-2 text-sm text-muted-foreground shadow-sm">
        <span>已同步</span>
        <strong class="text-foreground">{{ updatedLabel }}</strong>
      </div>
      <Button variant="outline" class="rounded-full" @click="emit('preview')">
        <BellRing class="size-4" />
        通知预览
      </Button>
      <Button class="rounded-full" @click="emit('refresh')">
        <RefreshCcw class="size-4" />
        刷新状态
      </Button>
    </div>
  </header>
</template>
