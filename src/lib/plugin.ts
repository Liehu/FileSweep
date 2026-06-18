import type { RouteRecordRaw } from "vue-router";

export type PluginType = "ui" | "system";

/** feature 类型（kunkun 启发预留）
 * - route: 进入路由（P1 全部此类型）
 * - template: 宿主渲染表单（P5 扩展）
 * - action: 纯命令无路由（P5 扩展）
 */
export type FeatureType = "route" | "template" | "action";

export interface PluginFeature {
  code: string;
  explain: string;
  cmds: string[];
  type?: FeatureType; // 默认 "route"
  route?: string; // type=route 时必填
}

export interface NavItem {
  label: string;
  icon: string; // lucide 图标名
  route: string;
  query?: Record<string, string>;
  badge?: () => string | number | undefined;
}

export interface NavGroup {
  title: string;
  items: NavItem[];
}

export interface PluginManifest {
  id: string;
  name: string;
  icon: string;
  pluginType: PluginType;
  features: PluginFeature[];
  navGroups?: NavGroup[];
  routes?: () => Promise<RouteRecordRaw[]>;
  onActivate?: (featureCode?: string) => void;
  /** 权限声明（默认 ["*"] 全权限，P1 内置可信）。P5 第三方插件显式声明 */
  permissions?: string[];
}

const registry = new Map<string, PluginManifest>();

export function definePlugin(m: PluginManifest): PluginManifest {
  if (registry.has(m.id)) {
    throw new Error(`plugin already registered: ${m.id}`);
  }
  registry.set(m.id, m);
  return m;
}

export function getPlugins(): PluginManifest[] {
  return Array.from(registry.values());
}

export function getPlugin(id: string): PluginManifest | undefined {
  return registry.get(id);
}

/** 命令面板搜索用的扁平化 feature */
export interface SearchableFeature {
  code: string;
  explain: string;
  cmds: string[];
  type: FeatureType;
  route?: string;
  pluginId: string;
  pluginName: string;
  pluginIcon: string;
}

export function getAllFeatures(): SearchableFeature[] {
  const result: SearchableFeature[] = [];
  for (const plugin of getPlugins()) {
    for (const feature of plugin.features) {
      result.push({
        code: feature.code,
        explain: feature.explain,
        cmds: feature.cmds,
        type: feature.type ?? "route",
        route: feature.route,
        pluginId: plugin.id,
        pluginName: plugin.name,
        pluginIcon: plugin.icon,
      });
    }
  }
  return result;
}
