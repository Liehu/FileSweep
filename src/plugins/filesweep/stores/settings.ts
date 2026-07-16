import { defineStore } from "pinia";
import { ref } from "vue";
import { pluginInvoke } from "@/lib/pluginInvoke";

export interface AiConfig {
  provider: string;
  ollama_url?: string;
  ollama_model?: string;
  openai_api_key?: string;
  openai_base_url?: string;
  openai_model?: string;
  claude_api_key?: string;
  claude_base_url?: string;
  custom_name?: string;
  custom_base_url?: string;
  custom_api_key?: string;
  custom_model?: string;
}

export interface PrivacyConfig {
  exclude_private: boolean;
  exclude_system: boolean;
  log_retention_days: number;
}

export interface RuleConfig {
  auto_categorize: boolean;
  auto_duplicate: boolean;
  keep_newest_version: boolean;
  move_to_recycle_bin: boolean;
  delete_empty_dirs: boolean;
}

export interface AppConfig {
  rules: RuleConfig;
  ai: AiConfig;
  privacy: PrivacyConfig;
  enable_github_search?: boolean;
  github_token?: string;
}

export const DEFAULT_RULES: RuleConfig = {
  auto_categorize: true,
  auto_duplicate: false,
  keep_newest_version: true,
  move_to_recycle_bin: true,
  delete_empty_dirs: false,
};

export const DEFAULT_AI: AiConfig = {
  provider: "offline",
};

export const DEFAULT_PRIVACY: PrivacyConfig = {
  exclude_private: true,
  exclude_system: true,
  log_retention_days: 30,
};

export const useSettingsStore = defineStore("settings", () => {
  const config = ref<AppConfig>({
    rules: { ...DEFAULT_RULES },
    ai: { ...DEFAULT_AI },
    privacy: { ...DEFAULT_PRIVACY },
  });
  const loading = ref(false);
  const error = ref<string | null>(null);
  const rules = ref<any[]>([]);

  async function fetchSettings() {
    loading.value = true;
    error.value = null;
    try {
      const res = await pluginInvoke<AppConfig>("filesweep", "settings:get");
      config.value = res;
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  async function updateSettings(data: Partial<AppConfig>) {
    try {
      await pluginInvoke("filesweep", "settings:update", data);
      config.value = { ...config.value, ...data };
    } catch (e) {
      error.value = String(e);
      throw e;
    }
  }

  // AI 配置连通性+认证测试。用当前表单值（不必先保存）发 ping 请求。
  // data: { provider, api_key, base_url, model } —— provider 决定走 OpenAI/Claude/Ollama 分支
  // 返回 { ok: boolean, model?, latency_ms?, error? }
  async function testConnection(data: Record<string, any>): Promise<{ ok: boolean; model?: string; latency_ms?: number; error?: string }> {
    return pluginInvoke("filesweep", "settings:test", data);
  }

  async function toggleRule(rule: keyof RuleConfig) {
    try {
      config.value.rules[rule] = !config.value.rules[rule];
      await pluginInvoke("filesweep", "settings:update", { rules: config.value.rules });
    } catch (e) {
      config.value.rules[rule] = !config.value.rules[rule];
      error.value = String(e);
    }
  }

  async function resetDefaults() {
    config.value.rules = { ...DEFAULT_RULES };
    config.value.ai = { ...DEFAULT_AI };
    config.value.privacy = { ...DEFAULT_PRIVACY };
    await updateSettings(config.value);
  }

  async function fetchRules() {
    try {
      const res = await pluginInvoke<{ categories: any[] }>("filesweep", "rules:get");
      rules.value = res.categories ?? res;
    } catch (e) {
      console.error("Failed to fetch rules:", e);
    }
  }

  async function resetDatabase() {
    await pluginInvoke("filesweep", "db:reset");
  }

  return {
    config, loading, error, rules,
    fetchSettings, updateSettings, toggleRule, resetDefaults,
    fetchRules, resetDatabase, testConnection,
    DEFAULT_RULES, DEFAULT_AI, DEFAULT_PRIVACY,
  };
});
