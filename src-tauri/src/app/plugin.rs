use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app::context::Context;

/// 插件类型（rubick 启发）
/// - Ui: 有界面，通过 features 关键词触发
/// - System: 无界面，启动时加载（P1 无 system 插件，预留）
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    Ui,
    System,
}

/// feature 类型（kunkun 启发预留）
/// - Route: 进入路由（P1 全部此类型）
/// - Template: 宿主渲染表单（P5 扩展）
/// - Action: 纯命令无路由（P5 扩展）
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FeatureType {
    Route,
    Template,
    Action,
}

impl Default for FeatureType {
    fn default() -> Self {
        FeatureType::Route
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PluginFeature {
    pub code: String,
    pub explain: String,
    pub cmds: Vec<String>,
    #[serde(default)]
    pub feature_type: FeatureType,
    /// Route 类型必填
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
}

/// 插件权限声明（kunkun 启发预留）
/// P1 内置插件默认 All，P5 第三方插件显式声明能力
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct PluginPermissions {
    /// ["*"] = 全权限；["fs:read","shell:exec"] = 细粒度
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl PluginPermissions {
    pub fn all() -> Self {
        Self {
            capabilities: vec!["*".to_string()],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub plugin_type: PluginType,
    pub features: Vec<PluginFeature>,
    pub version: String,
    #[serde(default = "PluginPermissions::all")]
    pub permissions: PluginPermissions,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin not found: {0}")]
    NotFound(String),
    #[error("unknown action: {0}")]
    UnknownAction(String),
    #[error("{0}")]
    Internal(String),
}

impl From<serde_json::Error> for PluginError {
    fn from(e: serde_json::Error) -> Self {
        PluginError::Internal(e.to_string())
    }
}

impl From<String> for PluginError {
    fn from(e: String) -> Self {
        PluginError::Internal(e)
    }
}

/// 插件 trait：所有内置插件实现此接口
#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;

    /// 声明支持的 action（校验/文档用）
    fn actions(&self) -> Vec<&'static str> {
        vec![]
    }

    /// 处理 invoke：action 为命令名（如 "scan:start"），args 为参数
    async fn invoke(&self, action: &str, args: Value, ctx: &Context) -> Result<Value, PluginError>;

    /// system 插件启动钩子（ui 插件默认空实现）
    async fn on_start(&self, _ctx: &Context) -> Result<(), PluginError> {
        Ok(())
    }
}
