import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";

const routes: RouteRecordRaw[] = [
  {
    path: "/",
    redirect: "/files",
  },
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
    path: "/tags",
    name: "Tags",
    component: () => import("@plugins/filesweep/views/TagsView.vue"),
    meta: { title: "标签管理" },
  },
  {
    path: "/categories",
    name: "Categories",
    component: () => import("@plugins/filesweep/views/CategoriesView.vue"),
    meta: { title: "分类管理" },
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

const router = createRouter({
  history: createWebHistory(),
  routes,
});

router.beforeEach((to, _from, next) => {
  const title = to.meta.title as string | undefined;
  if (title) {
    document.title = `${title} - FileSweep`;
  }
  next();
});

export default router;
