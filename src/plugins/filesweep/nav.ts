import type { NavGroup } from "@/lib/plugin";

export const navGroups: NavGroup[] = [
  {
    title: "文件",
    items: [
      { label: "全部文件", icon: "Folder", route: "/files", query: {} },
      { label: "重复文件", icon: "Copy", route: "/files", query: { dup: "1" } },
      { label: "多版本", icon: "Layers", route: "/files", query: { mv: "1" } },
    ],
  },
  {
    title: "工具",
    items: [
      { label: "扫描", icon: "Scan", route: "/scan" },
      { label: "软件目录", icon: "BookOpen", route: "/catalog" },
      { label: "AI丰富", icon: "Sparkles", route: "/enrich" },
      { label: "规则管理", icon: "Settings2", route: "/config" },
      { label: "操作日志", icon: "ScrollText", route: "/logs" },
      { label: "设置", icon: "Settings", route: "/settings" },
    ],
  },
];
