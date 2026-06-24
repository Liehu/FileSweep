import { definePlugin } from "@/lib/plugin";
import { routes } from "./routes";
import { navGroups } from "./nav";

export default definePlugin({
  id: "appmover",
  name: "软件迁移",
  icon: "HardDriveDownload",
  pluginType: "ui",
  features: [
    { code: "migrate", explain: "目录迁移", route: "/am/migrate", cmds: ["迁移", "migrate"] },
    { code: "monitor", explain: "目录监控", route: "/am/monitor", cmds: ["监控", "monitor"] },
    { code: "envvar", explain: "环境变量", route: "/am/envvar", cmds: ["环境变量", "envvar"] },
    { code: "baseline", explain: "基线管理", route: "/am/baseline", cmds: ["基线", "baseline"] },
    { code: "history", explain: "迁移历史", route: "/am/history", cmds: ["历史", "history"] },
  ],
  navGroups,
  routes: () => Promise.resolve(routes),
  permissions: ["*"],
});
