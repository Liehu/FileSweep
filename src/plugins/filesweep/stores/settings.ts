import { defineStore } from "pinia";
import { ref } from "vue";
import { invoke } from "@/lib/api";

export interface AiConfig {
  provider: string;
  ollama_url?: string;
  ollama_model?: string;
  openai_api_key?: string;
  openai_base_url?: string;
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
      const res = await invoke<AppConfig>("get_settings");
      config.value = res;
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  async function updateSettings(data: Partial<AppConfig>) {
    try {
      await invoke("update_settings", data);
      config.value = { ...config.value, ...data };
    } catch (e) {
      error.value = String(e);
      throw e;
    }
  }

  async function toggleRule(rule: keyof RuleConfig) {
    try {
      config.value.rules[rule] = !config.value.rules[rule];
      await invoke("update_settings", { rules: config.value.rules });
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
      const res = await invoke<{ categories: any[] }>("get_rules");
      rules.value = res.categories ?? res;
    } catch (e) {
      console.error("Failed to fetch rules:", e);
    }
  }

  async function resetDatabase() {
    await invoke("reset_db");
  }

  return {
    config, loading, error, rules,
    fetchSettings, updateSettings, toggleRule, resetDefaults,
    fetchRules, resetDatabase, DEFAULT_RULES, DEFAULT_AI, DEFAULT_PRIVACY,
  };
});
