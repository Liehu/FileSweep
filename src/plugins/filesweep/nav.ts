import type { NavGroup } from "@/lib/plugin";

export const navGroups: NavGroup[] = [
  {
    // 文件分类视图已移至右侧面板，左侧只保留功能菜单
    title: "工具",
    items: [
      { label: "扫描", icon: "Scan", route: "/scan" },
      { label: "全部文件", icon: "Folder", route: "/files" },
      { label: "软件目录", icon: "BookOpen", route: "/catalog" },
      { label: "智能建议", icon: "Lightbulb", route: "/suggestions" },
      { label: "规则管理", icon: "Settings2", route: "/config" },
      { label: "操作日志", icon: "ScrollText", route: "/logs" },
      { label: "设置", icon: "Settings", route: "/settings" },
    ],
  },
];
