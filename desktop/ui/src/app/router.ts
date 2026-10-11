import { createRouter, createWebHashHistory } from "vue-router";
import { Files, RadioTower, Settings2, Activity } from "lucide-vue-next";
const routes = [
      {
        path: "/",
        redirect: "/remote",
      },
      {
        path: "/remote",
        component: ()=> import('@/pages/RemoteAssistPage.vue'),
        icon: RadioTower,
        title:'远程',
      },
      {
        path: "/file-transfer",
        component: () => import('@/pages/PlaceholderPage.vue'),
        props: { title: '文件传输', description: '当前版本尚未开放文件传输。' },
        icon: Files,
        title:'文件传输',
      },
      {
        path: "/diagnostics",
        component: () => import('@/pages/DiagnosticsPage.vue'),
        icon: Activity,
        title: '诊断',
      },
      {
        path: "/settings",
        component: () => import('@/pages/SettingsPage.vue'),
        icon: Settings2,
        title: '设置'
      },
];
export const router = createRouter({
  history: createWebHashHistory(),
  routes
});

export const routerAndIcon = routes
    .filter(r=>r.icon && r.path !== '/file-transfer')
    .map(r => ({path: r.path,title: r.title, icon:r.icon}));
