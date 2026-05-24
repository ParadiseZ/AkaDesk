<script setup lang="ts">
import { CircleAlert, CircleCheckBig, Clock3 } from "lucide-vue-next";
import type { ShellMessage } from "@/types/shell";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

defineProps<{
  messages: ShellMessage[];
}>();

function formatTime(timestamp: number) {
  return new Date(timestamp).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function messageTone(level: string) {
  return level === "warning"
    ? "border-amber-200/80 bg-amber-50/90"
    : "border-emerald-100 bg-emerald-50/60";
}

function iconFor(level: string) {
  return level === "warning" ? CircleAlert : CircleCheckBig;
}
</script>

<template>
  <Card class="h-full rounded-[28px]">
    <CardHeader class="pb-3">
      <div class="flex items-start justify-between gap-3">
        <div class="space-y-1">
          <Badge variant="secondary" class="rounded-full px-3 py-1 text-[11px] uppercase tracking-[0.2em]">
            消息桥
          </Badge>
          <CardTitle>Host 与桌面壳消息</CardTitle>
          <CardDescription>这里保留 Host Runtime 诊断事件和桌面壳操作反馈。</CardDescription>
        </div>
        <Badge variant="outline" class="rounded-full px-3 py-1">
          {{ messages.length }} 条
        </Badge>
      </div>
    </CardHeader>
    <CardContent class="h-[calc(100vh-14rem)] p-0">
      <ScrollArea class="h-full px-6 pb-6">
        <div class="grid gap-3">
          <article
            v-for="entry in messages.slice().reverse()"
            :key="`${entry.code}-${entry.createdAtMs}`"
            :class="cn('rounded-2xl border p-4 shadow-sm', messageTone(entry.level))"
          >
            <div class="flex items-center justify-between gap-3">
              <div class="flex items-center gap-2">
                <component :is="iconFor(entry.level)" class="size-4" />
                <strong class="text-sm">{{ entry.code }}</strong>
              </div>
              <span class="inline-flex items-center gap-1 text-xs text-muted-foreground">
                <Clock3 class="size-3.5" />
                {{ formatTime(entry.createdAtMs) }}
              </span>
            </div>
            <p class="mt-3 text-sm leading-6 text-foreground/86">{{ entry.message }}</p>
            <div class="mt-3 flex items-center justify-between text-xs uppercase tracking-[0.18em] text-muted-foreground">
              <span>{{ entry.source }}</span>
              <span>{{ entry.level }}</span>
            </div>
          </article>
        </div>
      </ScrollArea>
    </CardContent>
  </Card>
</template>
