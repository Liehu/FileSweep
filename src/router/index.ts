import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";
import "@/plugins/_registry"; // 副作用：注册所有插件
import { getPlugins } from "@/lib/plugin";

async function buildRoutes(): Promise<RouteRecordRaw[]> {
  const allRoutes: RouteRecordRaw[] = [{ path: "/", redirect: "/files" }];
  for (const plugin of getPlugins()) {
    if (plugin.pluginType === "ui" && plugin.routes) {
      const pluginRoutes = await plugin.routes();
      allRoutes.push(...pluginRoutes);
    }
  }
  return allRoutes;
}

const router = createRouter({
  history: createWebHistory(),
  routes: [],
});

// 异步填充路由
buildRoutes().then((routes) => {
  for (const r of routes) {
    router.addRoute(r);
  }
});

router.beforeEach((to, _from, next) => {
  const title = to.meta.title as string | undefined;
  if (title) {
    document.title = `${title} - FileSweep`;
  }
  next();
});

export default router;
