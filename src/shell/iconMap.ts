import {
  Folder,
  FolderOpen,
  Search,
  Scan,
  Tag,
  BookOpen,
  Sparkles,
  Lightbulb,
  Settings,
  Settings2,
  ScrollText,
  ChevronLeft,
  ChevronRight,
  Copy,
  Layers,
  Menu,
  Minus,
  Square,
  X,
} from "lucide-vue-next";
import type { Component } from "vue";

/** lucide 图标名 → 组件映射，供 Sidebar/CommandPalette 动态渲染 */
export const iconMap: Record<string, Component> = {
  Folder,
  FolderOpen,
  Search,
  Scan,
  Tag,
  BookOpen,
  Sparkles,
  Lightbulb,
  Settings,
  Settings2,
  ScrollText,
  ChevronLeft,
  ChevronRight,
  Copy,
  Layers,
  Menu,
  Minus,
  Square,
  X,
};

export function getIcon(name: string): Component {
  return iconMap[name] ?? Folder;
}
