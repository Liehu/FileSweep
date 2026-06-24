import type { NavGroup } from "@/lib/plugin";

export const navGroups: NavGroup[] = [
  {
    title: "软件迁移",
    items: [
      { label: "目录迁移", icon: "HardDriveDownload", route: "/am/migrate" },
      { label: "目录监控", icon: "Radar", route: "/am/monitor" },
      { label: "迁移历史", icon: "History", route: "/am/history" },
    ],
  },
  {
    title: "系统维护",
    items: [
      { label: "环境变量", icon: "Variable", route: "/am/envvar" },
      { label: "基线管理", icon: "ShieldCheck", route: "/am/baseline" },
    ],
  },
];
