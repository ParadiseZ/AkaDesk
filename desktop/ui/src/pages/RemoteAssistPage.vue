<script setup lang="ts">
import { computed } from "vue";
import { Copy, EllipsisVertical, RefreshCcw } from "lucide-vue-next";
import type { AcceptableValue } from "reka-ui";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useShellStore } from "@/stores/shell";
import type { HostSettingsInput, VerificationMode } from "@/types/shell";

const store = useShellStore();
const verificationOptions = [
  { value: "TEMPORARY_CODE", label: "临时" },
  { value: "CUSTOM_CODE", label: "自定义" },
  { value: "BOTH_CODES", label: "使用两种" },
] as const;
const hostIdentity = computed(() => store.snapshot?.hostOverview.identity);
const hostSettings = computed(() => store.hostSettings);
const selectedVerificationMode = computed(() => hostSettings.value?.verificationMode ?? "TEMPORARY_CODE");
const verificationModeLabel = computed(() => verificationOptions.find(option => option.value === selectedVerificationMode.value)?.label);
const verificationLabel = computed(() => !store.snapshot?.ready ? "--" : selectedVerificationMode.value === "CUSTOM_CODE"
  ? (hostSettings.value?.customVerificationCode ? "已设置自定义验证码" : "请先在设置中添加")
  : hostSettings.value?.temporaryVerificationCode);
const formattedLocalDeviceId = computed(() => (hostIdentity.value?.deviceId ?? "").replace(/(\d{3})(?=\d)/g, "$1 "));
const persistHostSettings = (patch: Partial<HostSettingsInput>) => store.updateHostSettings(patch);
function onAllowRemoteAssistanceChange(checked: boolean) { void persistHostSettings({ allowRemoteAssistance: checked }); }
function onVerificationModeChange(value: AcceptableValue) {
  if (typeof value === "string") void persistHostSettings({ verificationMode: value as VerificationMode });
}
function onRefreshAfterEndChange(enabled: boolean) { void persistHostSettings({ refreshTemporaryCodeAfterControlledEnd: enabled }); }
function refreshTemporaryCode() { void store.refreshTemporaryVerificationCode(); }
async function copyLocalDeviceInfo() {
  const settings = hostSettings.value;
  if (!hostIdentity.value || !settings || !store.snapshot?.ready) return;
  const code = settings.verificationMode === "CUSTOM_CODE" ? settings.customVerificationCode : settings.temporaryVerificationCode;
  try { await navigator.clipboard.writeText('设备 ID: ' + hostIdentity.value.deviceId + '\n验证码: ' + code); }
  catch { store.errorMessage = "无法复制，请手动选中设备 ID 和验证码"; }
}
</script>

<template>
  <section class="grid min-h-full content-start">
    <header class="border-b border-slate-300 px-6 py-5">
      <h2 class="text-2xl font-semibold tracking-tight">远程</h2>
    </header>

    <div class="px-6 py-5">
      <p v-if="!store.snapshot?.ready" class="mb-4 text-sm text-slate-500">请启动 AkaDesk 桌面应用查看本机连接信息。</p>
      <Tabs default-value="remote-assist" class="grid gap-4">
        <TabsList class="h-auto gap-0 border-slate-300">
          <TabsTrigger value="remote-assist" class="h-11 px-5 data-[state=active]:border-sky-500 data-[state=active]:text-slate-900">远程协助</TabsTrigger>
        </TabsList>

        <TabsContent value="remote-assist" class="mt-0">
          <div class="border border-slate-300 bg-white">
            <section class="flex items-center justify-between border-b border-slate-300 px-5 py-4">
              <div>
                <div class="text-sm font-medium">允许他人协助</div>
                <p class="mt-1 text-xs text-slate-500">关闭后会立即结束当前屏幕共享。</p>
              </div>
              <Switch
                :disabled="!store.snapshot?.ready"
                :model-value="hostSettings?.allowRemoteAssistance"
                class="data-[state=unchecked]:border-slate-400 data-[state=unchecked]:bg-slate-300 data-[state=checked]:border-sky-500 data-[state=checked]:bg-sky-500"
                @update:model-value="onAllowRemoteAssistanceChange"
              />
            </section>

            <section class="border-b border-slate-300 px-5 py-4">
              <div class="grid gap-3 md:grid-cols-[minmax(220px,1fr)_140px_170px_88px_96px] md:items-end md:justify-between">
                <div class="min-w-0">
                  <div class="text-xs text-slate-500">本设备 ID</div>
                  <div class="mt-2 break-all font-mono text-base font-semibold tracking-wide">{{ formattedLocalDeviceId || "--" }}</div>
                </div>

                <div>
                  <div class="text-xs text-slate-500">验证码</div>
                  <div class="mt-2 flex h-9 max-w-[140px] items-center border border-slate-300 px-3 text-sm font-medium tracking-[0.12em]">
                    {{ verificationLabel }}
                  </div>
                </div>

                <div>
                  <div class="text-xs text-slate-500">验证方式</div>
                  <Select :disabled="!store.snapshot?.ready" :model-value="selectedVerificationMode" @update:model-value="onVerificationModeChange">
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
                  <Button variant="outline" class="mt-2 h-9 w-full rounded-none border-slate-300 px-3" :disabled="!store.snapshot?.ready" @click="copyLocalDeviceInfo">
                    <Copy class="size-4" />
                  </Button>
                </div>
              </div>

              <p class="mt-3 text-xs text-slate-500">
                使用“两种”时，临时验证码和自定义验证码任意一种均可连接。
              </p>
            </section>

            <section class="px-5 py-4 text-sm text-slate-500">
              支持 Android 查看电脑画面、接收电脑声音、操作鼠标键盘和调整显示设置。
            </section>
          </div>
        </TabsContent>

      </Tabs>
    </div>


  </section>
</template>
