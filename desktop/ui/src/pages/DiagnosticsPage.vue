<script setup lang="ts">
import { computed, ref } from "vue";
import { Button } from "@/components/ui/button";
import { desktopApi } from "@/services/desktopApi";
import { useShellStore } from "@/stores/shell";
const store = useShellStore();
const filter = ref("issues");
const exporting = ref(false);
const exportPath = ref("");
const exportError = ref("");
const status = computed(() => !store.snapshot?.ready ? "服务连接中断" : store.snapshot.diagnostics.degraded ? "连接异常，等待恢复" : store.snapshot.activeSession ? "正在共享屏幕" : "已准备就绪");
const summaries = computed(() => {
  const s = store.snapshot?.diagnostics;
  return [
    {label:"最近错误", entry:s?.lastError}, {label:"最近提醒", entry:s?.lastWarning}, {label:"最近连接异常", entry:s?.lastDegradedEvent},
    {label:"显示恢复", entry:s?.lastDisplayRestoreResult}, {label:"声音恢复", entry:s?.lastAudioRestoreResult},
    {label:"最近连接结束", entry:s?.lastSessionClosedEvent},
  ];
});
const entries = computed(() => store.messages.filter(e => filter.value === "all" || e.level === "error" || e.level === "warning").slice().sort((a,b)=>b.lastSeenMs-a.lastSeenMs));
function time(ms:number) { return new Date(ms).toLocaleString("zh-CN",{hour12:false}); }
async function exportReport() {
  exporting.value=true; exportError.value="";
  try { exportPath.value=await desktopApi.exportDiagnostics(); }
  catch(error) { exportError.value=error instanceof Error ? error.message : String(error); }
  finally {exporting.value=false;}
}
async function copyPath() {
  try {await navigator.clipboard.writeText(exportPath.value);}
  catch {exportError.value="无法复制，请手动选中文件路径";}
}
</script>

<template>
  <section class="grid min-h-full content-start">
    <header class="flex items-center justify-between border-b border-slate-300 px-6 py-5">
      <h2 class="text-2xl font-semibold tracking-tight">诊断</h2>
      <Button variant="outline" :disabled="exporting" @click="exportReport">{{ exporting ? "正在导出…" : "导出诊断" }}</Button>
    </header>
    <div class="grid gap-5 px-6 py-5">
      <div class="border px-4 py-3" :class="store.snapshot?.diagnostics.degraded || !store.snapshot?.ready ? 'border-amber-300 bg-amber-50' : 'border-slate-300 bg-white'">
        <strong class="text-sm">{{ status }}</strong>
        <p class="mt-1 text-xs text-slate-500">仅记录关键变化、失败与恢复，重复事件合并计数。</p>
      </div>
      <div v-if="exportPath" class="border border-slate-300 p-4 text-sm" role="status">
        <p>诊断已保存</p><p class="mt-2 select-all break-all text-xs text-slate-500">{{ exportPath }}</p>
        <Button variant="outline" class="mt-3 h-8" @click="copyPath">复制路径</Button>
      </div>
      <p v-if="exportError" class="text-sm text-red-700" role="alert">{{ exportError }}</p>
      <div class="grid gap-3 md:grid-cols-2">
        <article v-for="summary in summaries" :key="summary.label" class="border border-slate-300 p-4">
          <h3 class="text-xs text-slate-500">{{ summary.label }}</h3>
          <p class="mt-2 text-sm">{{ summary.entry?.message ?? '暂无记录' }}</p>
          <p v-if="summary.entry?.suggestedAction" class="mt-2 text-xs text-slate-500">{{ summary.entry.suggestedAction }}</p>
          <span v-if="summary.entry" class="mt-2 block text-xs text-slate-400">{{ time(summary.entry.lastSeenMs) }}</span>
        </article>
      </div>
      <section class="border border-slate-300">
        <div class="flex items-center justify-between border-b border-slate-300 px-4 py-3">
          <h3 class="text-sm font-medium">最近事件</h3>
          <select v-model="filter" aria-label="事件筛选" class="border border-slate-300 bg-white px-2 py-1 text-sm">
            <option value="issues">错误与提醒</option><option value="all">全部关键事件</option>
          </select>
        </div>
        <p v-if="!entries.length" class="px-4 py-5 text-sm text-slate-500">{{ filter === 'issues' ? '暂无错误或提醒' : '暂无记录' }}</p>
        <article v-for="entry in entries" :key="entry.eventId" class="border-b border-slate-200 px-4 py-3 last:border-0">
          <div class="flex items-center justify-between gap-3 text-sm">
            <span :class="entry.level==='error' ? 'text-red-700' : 'text-slate-800'">{{ entry.message }}</span>
            <span v-if="entry.repeatCount>1" class="shrink-0 text-xs text-slate-500">{{ entry.repeatCount }} 次</span>
          </div>
          <p v-if="entry.suggestedAction" class="mt-1 text-xs text-slate-500">{{ entry.suggestedAction }}</p>
          <time class="mt-2 block text-xs text-slate-400">{{ time(entry.lastSeenMs) }}</time>
        </article>
      </section>
    </div>
  </section>
</template>
