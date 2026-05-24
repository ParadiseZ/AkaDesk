import { createRouter, createWebHashHistory } from "vue-router";
import { Files, RadioTower, Settings2 } from "lucide-vue-next";
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
        icon: Files,
        title:'文件传输',
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
    .filter(r=>r.icon)
    .map(r => ({path: r.path,title: r.title, icon:r.icon}));