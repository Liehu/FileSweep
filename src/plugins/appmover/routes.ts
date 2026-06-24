import type { RouteRecordRaw } from "vue-router";

export const routes: RouteRecordRaw[] = [
  {
    path: "/am/migrate",
    name: "AmMigrate",
    component: () => import("@plugins/appmover/views/MigrateView.vue"),
    meta: { title: "目录迁移" },
  },
  {
    path: "/am/monitor",
    name: "AmMonitor",
    component: () => import("@plugins/appmover/views/MonitorView.vue"),
    meta: { title: "目录监控" },
  },
  {
    path: "/am/history",
    name: "AmHistory",
    component: () => import("@plugins/appmover/views/HistoryView.vue"),
    meta: { title: "迁移历史" },
  },
  {
    path: "/am/envvar",
    name: "AmEnvVar",
    component: () => import("@plugins/appmover/views/EnvVarView.vue"),
    meta: { title: "环境变量" },
  },
  {
    path: "/am/baseline",
    name: "AmBaseline",
    component: () => import("@plugins/appmover/views/BaselineView.vue"),
    meta: { title: "基线管理" },
  },
];
