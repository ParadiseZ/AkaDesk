import { createApp } from "vue";
import { createPinia } from "pinia";
import AppShell from "@/app/AppShell.vue";
import { router } from "./app/router";
import "./styles/globals.css";

createApp(AppShell).use(createPinia()).use(router).mount("#app");
