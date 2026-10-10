<script setup lang="ts">
defineProps<{
  title: string;
  description: string;
}>();
</script>

<template>
  <section class="grid min-h-screen content-start">
    <header class="border-b border-slate-300 px-6 py-5">
      <h2 class="text-2xl font-semibold tracking-tight">{{ title }}</h2>
      <p class="mt-2 text-sm text-slate-500">{{ description }}</p>
    </header>

    <div class="px-6 py-6">
      <div class="border border-slate-300 bg-slate-50 px-5 py-4 text-sm text-slate-500">
        此功能尚未开放。
      </div>
    </div>
  </section>
</template>
