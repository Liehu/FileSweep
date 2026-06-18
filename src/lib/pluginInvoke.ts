import { invoke } from "@/lib/api";

/** 统一插件调用：pluginInvoke("filesweep", "scan:stats", {...}) */
export function pluginInvoke<T = any>(
  plugin: string,
  action: string,
  args?: Record<string, any>,
): Promise<T> {
  return invoke<T>("plugin_invoke", { plugin, action, args });
}
