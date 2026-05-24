<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { Copy, EllipsisVertical, Laptop, Monitor, RefreshCcw, Smartphone, Tablet } from "lucide-vue-next";
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
import { useShellStore } from "@/stores/shell";
import type { HostSettingsInput } from "@/types/shell";

type DirectoryTab = "discover" | "history" | "favorites" | "paired";
type PlatformKind = "windows" | "android" | "linux" | "macos";
type VerificationMode = "TEMPORARY_CODE" | "CUSTOM_CODE" | "BOTH_CODES";

interface RemoteDirectoryRecord {
  key: string;
  deviceId: string;
  username: string;
  platform: PlatformKind;
}

const store = useShellStore();
const verificationOptions = [
  { value: "TEMPORARY_CODE", label: "临时" },
  { value: "CUSTOM_CODE", label: "自定义" },
  { value: "BOTH_CODES", label: "使用两种" },
] as const;

const directoryLists = reactive<Record<Exclude<DirectoryTab, "discover">, RemoteDirectoryRecord[]>>({
  history: [
    { key: "history-1", deviceId: "855104224", username: "Neo7", platform: "android" },
    { key: "history-2", deviceId: "472880611", username: "ThinkPad", platform: "windows" },
  ],
  favorites: [
    { key: "favorite-1", deviceId: "905441277", username: "Studio-PC", platform: "windows" },
    { key: "favorite-2", deviceId: "332108515", username: "Surface", platform: "windows" },
  ],
  paired: [
    { key: "paired-1", deviceId: "855104224", username: "Neo7", platform: "android" },
    { key: "paired-2", deviceId: "302118067", username: "Office-Mac", platform: "macos" },
  ],
});

const remoteDeviceId = ref("");
const connectDialogOpen = ref(false);
const dialogRemoteDeviceId = ref("");
const dialogAccessCode = ref("");
const selectedVerificationMode = ref<VerificationMode>("TEMPORARY_CODE");

const hostIdentity = computed(() => store.snapshot?.hostOverview.identity ?? null);
const hostSettings = computed(() => store.hostSettings);
const verificationMode = computed<VerificationMode>(
  () => (hostSettings.value?.verificationMode as VerificationMode) ?? "TEMPORARY_CODE",
);
const verificationModeLabel = computed(
  () => verificationOptions.find((option) => option.value === selectedVerificationMode.value)?.label ?? "临时",
);

const verificationLabel = computed(() => {
  if (!hostSettings.value) {
    return "--";
  }
  if (verificationMode.value === "CUSTOM_CODE") {
    return hostSettings.value.customVerificationCode.trim() ? "已设置自定义验证码" : "未设置自定义验证码";
  }
  return hostSettings.value.temporaryVerificationCode.slice(0, 8);
});

const formattedLocalDeviceId = computed(() => formatDeviceId(hostIdentity.value?.deviceId ?? ""));

watch(
  verificationMode,
  (value) => {
    selectedVerificationMode.value = value;
  },
  { immediate: true },
);

async function persistHostSettings(patch: Partial<HostSettingsInput>) {
  if (!hostSettings.value) {
    return;
  }
  await store.updateHostSettings({
    ...hostSettings.value,
    ...patch,
  });
}

function formatDeviceId(value: string) {
  return value
    .replace(/\s+/g, "")
    .replace(/(\d{3})(?=\d)/g, "$1 ")
    .trim();
}

function normalizeRemoteDeviceId(value: string | number | undefined) {
  return String(value ?? "")
    .replace(/\D+/g, "")
    .slice(0, 9);
}

function platformIcon(platform: PlatformKind) {
  if (platform === "android") return Smartphone;
  if (platform === "macos") return Tablet;
  if (platform === "linux") return Laptop;
  return Monitor;
}

function secondaryActionLabel(tab: Exclude<DirectoryTab, "discover">) {
  if (tab === "favorites") return "取消收藏";
  if (tab === "paired") return "取消配对";
  return "删除";
}

function openConnectDialog(targetDeviceId: string) {
  dialogRemoteDeviceId.value = normalizeRemoteDeviceId(targetDeviceId);
  dialogAccessCode.value = "";
  connectDialogOpen.value = true;
}

function connectFromRow(targetDeviceId: string) {
  remoteDeviceId.value = normalizeRemoteDeviceId(targetDeviceId);
  openConnectDialog(targetDeviceId);
}

function submitConnectRequest() {
  if (!remoteDeviceId.value.trim()) return;
  openConnectDialog(remoteDeviceId.value);
}

async function confirmConnect() {
  if (!dialogRemoteDeviceId.value.trim()) return;
  await store.queueRemoteConnect({
    remoteDeviceId: dialogRemoteDeviceId.value.trim(),
    accessCode: dialogAccessCode.value.trim(),
  });
  connectDialogOpen.value = false;
}

async function removeRecord(tab: Exclude<DirectoryTab, "discover">, key: string) {
  directoryLists[tab] = directoryLists[tab].filter((record) => record.key !== key);
}

async function copyLocalDeviceInfo() {
  if (!hostIdentity.value || !hostSettings.value) return;
  const text = [
    `设备ID: ${hostIdentity.value.deviceId}`,
    `临时验证码: ${hostSettings.value.temporaryVerificationCode}`,
    `自定义验证码: ${hostSettings.value.customVerificationCode}`,
  ].join("\n");
  await navigator.clipboard?.writeText(text);
}

function onAllowRemoteAssistanceChange(checked: boolean) {
  void persistHostSettings({ allowRemoteAssistance: checked });
}

function onVerificationModeChange(value: AcceptableValue) {
  if (typeof value !== "string") return;
  selectedVerificationMode.value = value as VerificationMode;
  void persistHostSettings({ verificationMode: value as VerificationMode });
}

function onRefreshAfterEndChange(enabled: boolean) {
  void persistHostSettings({ refreshTemporaryCodeAfterControlledEnd: enabled });
}

function refreshTemporaryCode() {
  void store.refreshTemporaryVerificationCode();
}

function onDialogCodeInput(value: string | number | undefined) {
  dialogAccessCode.value = String(value ?? "")
    .toUpperCase()
    .replace(/\s+/g, "")
    .slice(0, 8);
}

function onRemoteDeviceIdInput(value: string | number | undefined) {
  remoteDeviceId.value = normalizeRemoteDeviceId(value);
}
</script>

<template>
  <section class="grid min-h-full content-start">
    <header class="border-b border-slate-300 px-6 py-5">
      <h2 class="text-2xl font-semibold tracking-tight">远程</h2>
    </header>

    <div class="px-6 py-5">
      <Tabs default-value="remote-assist" class="grid gap-4">
        <TabsList class="h-auto gap-0 border-slate-300">
          <TabsTrigger value="remote-assist" class="h-11 px-5 data-[state=active]:border-sky-500 data-[state=active]:text-slate-900">远程协助</TabsTrigger>
          <TabsTrigger value="discover" class="h-11 px-5 data-[state=active]:border-sky-500 data-[state=active]:text-slate-900">发现</TabsTrigger>
          <TabsTrigger value="history" class="h-11 px-5 data-[state=active]:border-sky-500 data-[state=active]:text-slate-900">历史</TabsTrigger>
          <TabsTrigger value="favorites" class="h-11 px-5 data-[state=active]:border-sky-500 data-[state=active]:text-slate-900">收藏</TabsTrigger>
          <TabsTrigger value="paired" class="h-11 px-5 data-[state=active]:border-sky-500 data-[state=active]:text-slate-900">已配对</TabsTrigger>
        </TabsList>

        <TabsContent value="remote-assist" class="mt-0">
          <div class="border border-slate-300 bg-white">
            <section class="flex items-center justify-between border-b border-slate-300 px-5 py-4">
              <div>
                <div class="text-sm font-medium">允许他人协助</div>
                <p class="mt-1 text-xs text-slate-500">Host runtime 通过本地 IPC 保存协助开关。</p>
              </div>
              <Switch
                :checked="hostSettings?.allowRemoteAssistance"
                class="data-[state=unchecked]:border-slate-400 data-[state=unchecked]:bg-slate-300 data-[state=checked]:border-sky-500 data-[state=checked]:bg-sky-500"
                @update:checked="onAllowRemoteAssistanceChange"
              />
            </section>

            <section class="border-b border-slate-300 px-5 py-4">
              <div class="grid gap-3 md:grid-cols-[132px_140px_170px_88px_96px] md:items-end md:justify-between">
                <div class="max-w-[132px]">
                  <div class="text-xs text-slate-500">本设备 ID</div>
                  <div class="mt-2 text-2xl font-semibold tracking-[0.08em]">{{ formattedLocalDeviceId || "--" }}</div>
                </div>

                <div>
                  <div class="text-xs text-slate-500">验证码</div>
                  <div class="mt-2 flex h-9 max-w-[140px] items-center border border-slate-300 px-3 text-sm font-medium tracking-[0.12em]">
                    {{ verificationLabel }}
                  </div>
                </div>

                <div>
                  <div class="text-xs text-slate-500">验证方式</div>
                  <Select :model-value="selectedVerificationMode" @update:model-value="onVerificationModeChange">
                    <SelectTrigger class="mt-2 h-9 w-full rounded-none border-slate-300 bg-white">
                      <SelectValue>{{ verificationModeLabel }}</SelectValue>
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem v-for="option in verificationOptions" :key="option.value" :value="option.value">
                        {{ option.label }}
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div v-if="selectedVerificationMode === 'TEMPORARY_CODE'">
                  <div class="text-xs text-slate-500">设置</div>
                  <DropdownMenu>
                    <DropdownMenuTrigger as-child>
                      <Button variant="outline" class="mt-2 h-9 w-full rounded-none border-slate-300 bg-white px-0 hover:bg-slate-50">
                        <EllipsisVertical class="size-4 text-slate-500" />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="start" class="w-44 rounded-none border-slate-300 p-0">
                      <DropdownMenuItem class="rounded-none border-b border-slate-300 px-3 py-2 focus:bg-slate-100" @select="refreshTemporaryCode">
                        <RefreshCcw class="size-4" />
                        立即刷新
                      </DropdownMenuItem>
                      <DropdownMenuItem class="rounded-none border-b border-slate-300 px-3 py-2 focus:bg-slate-100" @select="onRefreshAfterEndChange(true)">
                        自动刷新：被控结束后
                      </DropdownMenuItem>
                      <DropdownMenuItem class="rounded-none px-3 py-2 focus:bg-slate-100" @select="onRefreshAfterEndChange(false)">
                        自动刷新：关闭
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                <div v-else></div>

                <div>
                  <div class="text-xs text-slate-500">复制</div>
                  <Button variant="outline" class="mt-2 h-9 w-full rounded-none border-slate-300 px-3" @click="copyLocalDeviceInfo">
                    <Copy class="size-4" />
                  </Button>
                </div>
              </div>

              <p class="mt-3 text-xs text-slate-500">
                使用“两种”时，Host runtime 会先校验临时验证码，再校验自定义验证码，两次都失败才算失败。
              </p>
            </section>

            <section class="grid gap-3 px-5 py-4 md:grid-cols-[160px_132px]">
              <div class="max-w-[160px]">
                <div class="text-xs text-slate-500">远程 ID</div>
                <Input
                  :model-value="remoteDeviceId"
                  class="mt-2 h-9 rounded-none border-slate-300"
                  inputmode="numeric"
                  maxlength="9"
                  placeholder="输入远程设备 ID"
                  @update:model-value="onRemoteDeviceIdInput"
                />
              </div>
              <div class="flex items-end">
                <Button class="h-9 w-full rounded-none bg-slate-900 px-4 hover:bg-slate-800" @click="submitConnectRequest">
                  连接
                </Button>
              </div>
            </section>
          </div>
        </TabsContent>

        <TabsContent value="discover" class="mt-0">
          <div class="border border-slate-300 bg-white px-5 py-4 text-sm text-slate-500">
            发现页暂时保留占位，后续接局域网发现结果。
          </div>
        </TabsContent>

        <TabsContent
          v-for="tab in (['history', 'favorites', 'paired'] as Exclude<DirectoryTab, 'discover'>[])"
          :key="tab"
          :value="tab"
          class="mt-0"
        >
          <div class="border border-slate-300 bg-white">
            <article
              v-for="record in directoryLists[tab]"
              :key="record.key"
              class="grid grid-cols-[minmax(0,1fr)_56px] border-b border-slate-300 last:border-b-0"
            >
              <button
                type="button"
                class="flex min-w-0 items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-slate-50"
                @click="connectFromRow(record.deviceId)"
              >
                <div class="grid size-9 place-items-center border border-slate-300 text-slate-600">
                  <component :is="platformIcon(record.platform)" class="size-4" />
                </div>
                <div class="min-w-0">
                  <div class="truncate text-sm font-medium">{{ record.deviceId }}</div>
                  <div class="mt-1 truncate text-xs text-slate-500">{{ record.username }}</div>
                </div>
              </button>

              <div class="border-l border-slate-300">
                <DropdownMenu>
                  <DropdownMenuTrigger as-child>
                    <button
                      type="button"
                      class="grid h-full w-full place-items-center transition-colors hover:bg-slate-50"
                    >
                      <EllipsisVertical class="size-4 text-slate-500" />
                    </button>
                  </DropdownMenuTrigger>

                  <DropdownMenuContent align="end" class="w-32 rounded-none border-slate-300 p-0">
                    <DropdownMenuItem class="rounded-none border-b border-slate-300 px-3 py-2 focus:bg-slate-100" @select="connectFromRow(record.deviceId)">
                      连接
                    </DropdownMenuItem>
                    <DropdownMenuItem class="rounded-none px-3 py-2 focus:bg-slate-100" @select="removeRecord(tab, record.key)">
                      {{ secondaryActionLabel(tab) }}
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </article>
          </div>
        </TabsContent>
      </Tabs>
    </div>

    <Dialog v-model:open="connectDialogOpen">
      <DialogContent class="max-w-[360px] rounded-none border-slate-300 p-0 sm:max-w-[360px]">
        <DialogHeader class="border-b border-slate-300 px-5 py-4">
          <DialogTitle class="text-sm font-medium">输入验证码</DialogTitle>
          <DialogDescription class="mt-1 text-xs text-slate-500">
            远程设备 ID：{{ formatDeviceId(dialogRemoteDeviceId) }}
          </DialogDescription>
        </DialogHeader>
        <div class="grid gap-4 px-5 py-4">
          <div>
            <div class="text-xs text-slate-500">验证码</div>
            <Input
              :model-value="dialogAccessCode"
              class="mt-2 rounded-none border-slate-300"
              maxlength="8"
              placeholder="输入验证码"
              @update:model-value="onDialogCodeInput"
              @keydown.enter="confirmConnect"
            />
          </div>
          <DialogFooter class="grid grid-cols-2 gap-3 sm:grid-cols-2">
            <Button variant="outline" class="h-9 rounded-none border-slate-300 px-4" @click="connectDialogOpen = false">
              取消
            </Button>
            <Button class="h-9 rounded-none bg-slate-900 px-4 hover:bg-slate-800" @click="confirmConnect">
              连接
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  </section>
</template>
