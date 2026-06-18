use std::collections::HashMap;

use serde_json::Value;

use crate::app::context::Context;
use crate::app::plugin::{Plugin, PluginError, PluginMetadata, PluginType};

/// 插件注册表：管理所有已注册插件，提供分发能力
pub struct PluginHost {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let id = plugin.metadata().id.clone();
        log::info!("registering plugin: {}", id);
        self.plugins.insert(id, plugin);
    }

    /// 返回所有插件元信息（plugin_list 命令用）
    pub fn metadata_list(&self) -> Vec<PluginMetadata> {
        self.plugins.values().map(|p| p.metadata().clone()).collect()
    }

    /// 启动所有 system 插件
    pub async fn start_system_plugins(&self, ctx: &Context) -> Result<(), PluginError> {
        for p in self.plugins.values() {
            if p.metadata().plugin_type == PluginType::System {
                log::info!("starting system plugin: {}", p.metadata().id);
                p.on_start(ctx).await?;
            }
        }
        Ok(())
    }

    /// 分发 invoke 到目标插件
    pub async fn dispatch(
        &self,
        plugin_id: &str,
        action: &str,
        args: Value,
        ctx: &Context,
    ) -> Result<Value, PluginError> {
        let plugin = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;
        plugin.invoke(action, args, ctx).await
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}
