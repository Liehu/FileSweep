import type { RouteRecordRaw } from "vue-router";

export const routes: RouteRecordRaw[] = [
  {
    path: "/files",
    name: "Files",
    component: () => import("@plugins/filesweep/views/FileListView.vue"),
    meta: { title: "文件管理" },
  },
  {
    path: "/scan",
    name: "Scan",
    component: () => import("@plugins/filesweep/views/ScanView.vue"),
    meta: { title: "扫描文件" },
  },
  {
    path: "/catalog",
    name: "Catalog",
    component: () => import("@plugins/filesweep/views/CatalogView.vue"),
    meta: { title: "文件目录" },
  },
  {
    path: "/enrich",
    name: "Enrich",
    component: () => import("@plugins/filesweep/views/EnrichView.vue"),
    meta: { title: "文件丰富" },
  },
  {
    path: "/config",
    name: "Config",
    component: () => import("@plugins/filesweep/views/ConfigView.vue"),
    meta: { title: "规则管理" },
  },
  {
    path: "/logs",
    name: "Logs",
    component: () => import("@plugins/filesweep/views/LogsView.vue"),
    meta: { title: "操作日志" },
  },
  {
    path: "/settings",
    name: "Settings",
    component: () => import("@plugins/filesweep/views/SettingsView.vue"),
    meta: { title: "设置" },
  },
];
