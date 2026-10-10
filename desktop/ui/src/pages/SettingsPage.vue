<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { RefreshCcw } from "lucide-vue-next";
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
const connectionMethodOptions = [
  { value: "ACCESS_CODE", label: "验证码" },
  { value: "TRUST_ONLY", label: "免密" },
  { value: "ACCESS_CODE_AND_CONSENT", label: "双重验证" },
] as const;

const shellForm = reactive({
  launchOnStartup: false,
  minimizeToTray: false,
  notificationsEnabled: false,
  lastRoute: "/remote",
  lastRemoteDeviceId: "",
  lastAccessCode: "",
});

const hostSettings = computed(() => store.hostSettings);
const hostIdentity = computed(() => store.snapshot?.hostOverview.identity ?? null);
const securityMethod = computed<ConnectionMethod>(
  () => (hostSettings.value?.connectionMethod as ConnectionMethod) ?? "ACCESS_CODE",
);
const selectedSecurityMethod = ref<ConnectionMethod>("ACCESS_CODE");
const securityMethodLabel = computed(
  () => connectionMethodOptions.find((option) => option.value === selectedSecurityMethod.value)?.label ?? "验证码",
);
const formattedLocalDeviceId = computed(() => formatDeviceId(hostIdentity.value?.deviceId ?? ""));
const temporaryVerificationCodeLabel = computed(() => hostSettings.value?.temporaryVerificationCode.slice(0, 8) ?? "--");
const customVerificationCodeLabel = computed(() => hostSettings.value?.customVerificationCode.trim() || "未设置");
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
    selectedSecurityMethod.value = (value.connectionMethod as ConnectionMethod) ?? "ACCESS_CODE";
    if (!customCodeDialogOpen.value) customCodeDraft.value = value.customVerificationCode;
  },
  { immediate: true },
);

function securityDescription(method: ConnectionMethod) {
  if (method === "TRUST_ONLY") return "不需验证码，同意后即可连接。";
  if (method === "ACCESS_CODE_AND_CONSENT") return "输入设备验证码后，还需本机同意。";
  return "输入设备验证码后即可连接。";
}

function formatDeviceId(value: string) {
  return value
    .replace(/\s+/g, "")
    .replace(/(\d{3})(?=\d)/g, "$1 ")
    .trim();
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
  return store.updateHostSettings(patch);
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
  selectedSecurityMethod.value = value as ConnectionMethod;
  void persistHostSettings({ connectionMethod: value as ConnectionMethod });
}

function openCustomCodeDialog() {
  customCodeDraft.value = hostSettings.value?.customVerificationCode ?? "";
  customCodeDialogOpen.value = true;
}

function refreshTemporaryCode() {
  void store.refreshTemporaryVerificationCode();
}

async function saveCustomCode() {
  if (!/^(?=.*[A-Z])(?=.*[0-9])[A-Z0-9]{6,8}$/.test(customCodeDraft.value)) { store.errorMessage = "自定义验证码须为 6–8 位字母和数字组合"; return; }
  if (await persistHostSettings({ customVerificationCode: customCodeDraft.value })) customCodeDialogOpen.value = false;
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
              <h3 class="text-lg font-medium">应用偏好</h3>
              <p class="mt-1 text-sm text-slate-500">启动页自动保存；尚未实现的选项已禁用。</p>
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
                  <div class="mt-1 text-xs text-slate-500">尚未实现，当前不可用。</div>
                </div>
                <Switch
                  :model-value="false" disabled
                  class="data-[state=unchecked]:border-slate-400 data-[state=unchecked]:bg-slate-300 data-[state=checked]:border-sky-500 data-[state=checked]:bg-sky-500"
                  @update:model-value="onLaunchOnStartupChange"
                />
              </label>

              <div class="flex items-center justify-between border border-slate-300 px-4 py-3">
                <div>
                  <div class="text-sm font-medium">最小化到托盘</div>
                  <div class="mt-1 text-xs text-slate-500">尚未实现，关闭窗口会退出应用。</div>
                </div>
                <Switch
                  :model-value="false" disabled
                  class="data-[state=unchecked]:border-slate-400 data-[state=unchecked]:bg-slate-300 data-[state=checked]:border-sky-500 data-[state=checked]:bg-sky-500"
                  @update:model-value="onMinimizeToTrayChange"
                />
              </div>

              <label class="flex items-center justify-between border border-slate-300 px-4 py-3">
                <div>
                  <div class="text-sm font-medium">启用桌面通知</div>
                  <div class="mt-1 text-xs text-slate-500">系统通知尚未实现；连接申请仍需在应用内确认。</div>
                </div>
                <Switch
                  :model-value="false" disabled
                  class="data-[state=unchecked]:border-slate-400 data-[state=unchecked]:bg-slate-300 data-[state=checked]:border-sky-500 data-[state=checked]:bg-sky-500"
                  @update:model-value="onNotificationsEnabledChange"
                />
              </label>
            </div>
          </div>
        </TabsContent>

        <TabsContent value="security" class="mt-0">
          <div class="border border-slate-300 bg-white">
            <div class="border-b border-slate-300 px-5 py-4">
              <h3 class="text-base font-medium">远程协助连接本设备的方式</h3>
              <p class="mt-1 text-sm text-slate-500">{{ securityDescription(selectedSecurityMethod) }}</p>
            </div>
            <section class="border-b border-slate-300 px-5 py-4">
              <div class="grid gap-3 xl:grid-cols-[minmax(220px,1.5fr)_160px_100px_minmax(140px,1fr)] xl:items-end">
                <div class="min-w-0">
                  <div class="text-xs text-slate-500">本设备 ID</div>
                  <div class="mt-2 break-all font-mono text-base font-semibold">{{ formattedLocalDeviceId || "--" }}</div>
                </div>

                <div class="max-w-[160px]">
                  <div class="text-xs text-slate-500">当前临时验证码</div>
                  <div class="mt-2 flex h-9 items-center border border-slate-300 px-3 text-sm font-medium tracking-[0.12em]">
                    {{ temporaryVerificationCodeLabel }}
                  </div>
                </div>

                <div>
                  <div class="text-xs text-slate-500">刷新</div>
                  <Button variant="outline" class="mt-2 h-9 w-full rounded-none border-slate-300 px-3" @click="refreshTemporaryCode">
                    <RefreshCcw class="mr-2 size-4" />
                    刷新
                  </Button>
                </div>

                <div>
                  <div class="text-xs text-slate-500">当前自定义验证码</div>
                  <div class="mt-2 flex h-9 items-center border border-slate-300 px-3 text-sm">
                    {{ customVerificationCodeLabel }}
                  </div>
                </div>
              </div>
              <p class="mt-3 text-xs text-slate-500">
                被控端日常连接至少需要把本设备 ID 告诉控制端；使用验证码连接时，再把当前验证码一并提供给对方。
              </p>
            </section>
            <div class="grid gap-4 px-5 py-4">
              <div class="max-w-[220px]">
                <div class="text-xs text-slate-500">连接方式</div>
                <Select :model-value="selectedSecurityMethod" @update:model-value="onConnectionMethodChange">
                  <SelectTrigger class="mt-2 h-9 w-full rounded-none border-slate-300 bg-white">
                    <SelectValue>{{ securityMethodLabel }}</SelectValue>
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem v-for="option in connectionMethodOptions" :key="option.value" :value="option.value">
                      {{ option.label }}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <div
                v-if="selectedSecurityMethod === 'ACCESS_CODE' || selectedSecurityMethod === 'ACCESS_CODE_AND_CONSENT'"
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
              placeholder="输入 6–8 位字母和数字组合"
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
