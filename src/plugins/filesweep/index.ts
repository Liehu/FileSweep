import { definePlugin } from "@/lib/plugin";
import { routes } from "./routes";
import { navGroups } from "./nav";

export default definePlugin({
  id: "filesweep",
  name: "文件整理",
  icon: "Folder",
  pluginType: "ui",
  features: [
    { code: "files", explain: "全部文件", route: "/files", cmds: ["文件", "files"] },
    { code: "scan", explain: "扫描文件", route: "/scan", cmds: ["扫描", "scan"] },
    { code: "dedup", explain: "重复文件", route: "/files", cmds: ["去重", "重复", "dedup"] },
    { code: "catalog", explain: "软件目录", route: "/catalog", cmds: ["目录", "catalog"] },
    { code: "enrich", explain: "AI 丰富", route: "/enrich", cmds: ["AI", "丰富", "enrich"] },
    { code: "suggestions", explain: "智能建议", route: "/suggestions", cmds: ["建议", "清理", "瘦身", "suggestions"] },
    { code: "search", explain: "搜索文件", route: "/files", cmds: ["搜索", "查找", "search"] },
    { code: "config", explain: "规则管理", route: "/config", cmds: ["配置", "规则", "config"] },
  ],
  navGroups,
  routes: () => Promise.resolve(routes),
  permissions: ["*"],
});
