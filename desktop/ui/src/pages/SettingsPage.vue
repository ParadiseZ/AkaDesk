<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import type { AcceptableValue } from "reka-ui";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { routerAndIcon } from "@/app/router";
import { useShellStore } from "@/stores/shell";

type ConnectionMethod = "ACCESS_CODE" | "TRUST_ONLY" | "ACCESS_CODE_AND_CONSENT";

const store = useShellStore();

const shellForm = reactive({
  launchOnStartup: false,
  minimizeToTray: true,
  notificationsEnabled: true,
  lastRoute: "/remote",
  lastRemoteDeviceId: "",
  lastAccessCode: "",
});

const hostSettings = computed(() => store.hostSettings);
const securityMethod = computed<ConnectionMethod>(
  () => (hostSettings.value?.connectionMethod as ConnectionMethod) ?? "ACCESS_CODE",
);
const routeOptions = computed(() =>
  routerAndIcon.map((item) => ({
    label: item.title,
    value: item.path,
  })),
);

const customCodeDialogOpen = ref(false);
const customCodeDraft = ref("");

watch(
  () => store.shellSettings,
  (value) => {
    if (!value) return;
    Object.assign(shellForm, value);
  },
  { immediate: true },
);

watch(
  hostSettings,
  (value) => {
    if (!value) return;
    customCodeDraft.value = value.customVerificationCode;
  },
  { immediate: true },
);

function securityDescription(method: ConnectionMethod) {
  if (method === "TRUST_ONLY") return "不需验证码，同意后即可连接。";
  if (method === "ACCESS_CODE_AND_CONSENT") return "设备 id + 验证码 + 同意可连接。";
  return "设备 id + 验证码连接，文档方案是这种。";
}

function onCustomCodeInput(value: string | number | undefined) {
  customCodeDraft.value = String(value ?? "")
    .toUpperCase()
    .replace(/[^A-Z0-9]/g, "")
    .slice(0, 8);
}

async function persistShellSettings(patch: Partial<typeof shellForm>) {
  const next = { ...shellForm, ...patch };
  Object.assign(shellForm, next);
  await store.saveShellSettings(next);
}

async function persistHostSettings(patch: Record<string, unknown>) {
  if (!hostSettings.value) return;
  await store.updateHostSettings({
    ...hostSettings.value,
    ...patch,
  });
}

function onLaunchOnStartupChange(checked: boolean) {
  void persistShellSettings({ launchOnStartup: checked });
}

function onMinimizeToTrayChange(checked: boolean) {
  void persistShellSettings({ minimizeToTray: checked });
}

function onNotificationsEnabledChange(checked: boolean) {
  void persistShellSettings({ notificationsEnabled: checked });
}

function onRouteChange(value: AcceptableValue) {
  if (typeof value !== "string") return;
  void persistShellSettings({ lastRoute: value });
}

function onConnectionMethodChange(value: AcceptableValue) {
  if (typeof value !== "string") return;
  void persistHostSettings({ connectionMethod: value as ConnectionMethod });
}

function openCustomCodeDialog() {
  customCodeDraft.value = hostSettings.value?.customVerificationCode ?? "";
  customCodeDialogOpen.value = true;
}

async function saveCustomCode() {
  await persistHostSettings({ customVerificationCode: customCodeDraft.value });
  customCodeDialogOpen.value = false;
}
</script>

<template>
  <section class="grid min-h-full content-start">
    <header class="border-b border-slate-300 px-6 py-5">
      <h2 class="text-2xl font-semibold tracking-tight">设置</h2>
    </header>

    <div class="px-6 py-5">
      <Tabs default-value="general" class="grid gap-4">
        <TabsList class="h-auto gap-0 border-slate-300">
          <TabsTrigger value="general" class="h-11 px-5 data-[state=active]:border-sky-500 data-[state=active]:text-slate-900">常规</TabsTrigger>
          <TabsTrigger value="security" class="h-11 px-5 data-[state=active]:border-sky-500 data-[state=active]:text-slate-900">安全</TabsTrigger>
        </TabsList>

        <TabsContent value="general" class="mt-0">
          <div class="border border-slate-300 bg-white">
            <div class="border-b border-slate-300 px-5 py-4">
              <h3 class="text-lg font-medium">桌面壳偏好</h3>
              <p class="mt-1 text-sm text-slate-500">此处修改直接异步保存到桌面壳本地设置。</p>
            </div>
            <div class="grid gap-4 px-5 py-4">
              <div class="max-w-[220px]">
                <div class="text-xs text-slate-500">启动页</div>
                <Select :model-value="shellForm.lastRoute" @update:model-value="onRouteChange">
                  <SelectTrigger class="mt-2 h-9 w-full rounded-none border-slate-300 bg-white">
                    <SelectValue placeholder="选择启动页" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem v-for="option in routeOptions" :key="option.value" :value="option.value">
                      {{ option.label }}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <label class="flex items-center justify-between border border-slate-300 px-4 py-3">
                <div>
                  <div class="text-sm font-medium">开机自启</div>
                  <div class="mt-1 text-xs text-slate-500">后续可直接挂到系统级启动项管理。</div>
                </div>
                <Switch
                  :checked="shellForm.launchOnStartup"
                  class="data-[state=unchecked]:border-slate-400 data-[state=unchecked]:bg-slate-300 data-[state=checked]:border-sky-500 data-[state=checked]:bg-sky-500"
                  @update:checked="onLaunchOnStartupChange"
                />
              </label>

              <div class="flex items-center justify-between border border-slate-300 px-4 py-3">
                <div>
                  <div class="text-sm font-medium">最小化到托盘</div>
                  <div class="mt-1 text-xs text-slate-500">关闭窗口时保留宿主壳常驻。</div>
                </div>
                <Switch
                  :checked="shellForm.minimizeToTray"
                  class="data-[state=unchecked]:border-slate-400 data-[state=unchecked]:bg-slate-300 data-[state=checked]:border-sky-500 data-[state=checked]:bg-sky-500"
                  @update:checked="onMinimizeToTrayChange"
                />
              </div>

              <label class="flex items-center justify-between border border-slate-300 px-4 py-3">
                <div>
                  <div class="text-sm font-medium">启用桌面通知</div>
                  <div class="mt-1 text-xs text-slate-500">决定是否显示协助申请和关键事件弹层。</div>
                </div>
                <Switch
                  :checked="shellForm.notificationsEnabled"
                  class="data-[state=unchecked]:border-slate-400 data-[state=unchecked]:bg-slate-300 data-[state=checked]:border-sky-500 data-[state=checked]:bg-sky-500"
                  @update:checked="onNotificationsEnabledChange"
                />
              </label>
            </div>
          </div>
        </TabsContent>

        <TabsContent value="security" class="mt-0">
          <div class="border border-slate-300 bg-white">
            <div class="border-b border-slate-300 px-5 py-4">
              <h3 class="text-base font-medium">远程协助连接本设备的方式</h3>
              <p class="mt-1 text-sm text-slate-500">{{ securityDescription(securityMethod) }}</p>
            </div>
            <div class="grid gap-4 px-5 py-4">
              <div class="max-w-[220px]">
                <div class="text-xs text-slate-500">连接方式</div>
                <Select :model-value="securityMethod" @update:model-value="onConnectionMethodChange">
                  <SelectTrigger class="mt-2 h-9 w-full rounded-none border-slate-300 bg-white">
                    <SelectValue placeholder="选择连接方式" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="ACCESS_CODE">验证码</SelectItem>
                    <SelectItem value="TRUST_ONLY">免密</SelectItem>
                    <SelectItem value="ACCESS_CODE_AND_CONSENT">双重验证</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div
                v-if="securityMethod === 'ACCESS_CODE' || securityMethod === 'ACCESS_CODE_AND_CONSENT'"
                class="border border-slate-300"
              >
                <div class="flex items-center justify-between px-4 py-3">
                  <div>
                    <div class="text-sm font-medium">自定义验证码</div>
                    <div class="mt-1 text-xs text-slate-500">当前 {{ hostSettings?.customVerificationCode || "未设置" }}</div>
                  </div>
                  <Button variant="outline" class="h-9 rounded-none border-slate-300 px-4" @click="openCustomCodeDialog">
                    修改
                  </Button>
                </div>
              </div>
            </div>
          </div>
        </TabsContent>
      </Tabs>
    </div>

    <Dialog v-model:open="customCodeDialogOpen">
      <DialogContent class="max-w-[360px] rounded-none border-slate-300 p-0 sm:max-w-[360px]">
        <DialogHeader class="border-b border-slate-300 px-5 py-4">
          <DialogTitle class="text-sm font-medium">修改自定义验证码</DialogTitle>
          <DialogDescription class="mt-1 text-xs text-slate-500">
            当前验证码：{{ hostSettings?.customVerificationCode || "未设置" }}
          </DialogDescription>
        </DialogHeader>
        <div class="grid gap-4 px-5 py-4">
          <div>
            <div class="text-xs text-slate-500">验证码</div>
            <Input
              :model-value="customCodeDraft"
              class="mt-2 rounded-none border-slate-300"
              maxlength="8"
              placeholder="输入 8 位以内数字或字母"
              @update:model-value="onCustomCodeInput"
              @keydown.enter="saveCustomCode"
            />
          </div>
          <DialogFooter class="grid grid-cols-2 gap-3 sm:grid-cols-2">
            <Button variant="outline" class="h-9 rounded-none border-slate-300 px-4" @click="customCodeDialogOpen = false">
              取消
            </Button>
            <Button class="h-9 rounded-none bg-slate-900 px-4 hover:bg-slate-800" @click="saveCustomCode">
              保存
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  </section>
</template>
