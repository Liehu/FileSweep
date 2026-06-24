//! AppMover 插件：辅助用户迁移 AppData / Program Files 下的非系统默认目录。
//!
//! 设计要点（见 docs / grill 结论）：
//! - 只迁"目录"，不迁散文件；仅当前用户；ProgramData 不迁。
//! - 系统默认识别 = 强白名单 ∪ 纯净VM基线 → 保护集；其余为候选迁移集。
//! - 迁移机制 = 方案 X（真身在 D:）+ Junction + 方案 P（复制→校验→建链接→删源）。
//! - 占用检测：进程驱动杀目标进程 → 模块反查（重启 explorer）→ 标记手动。
//! - 运行形态：管理员态 GUI + 托盘常驻 + 轮询监控（不做 Windows 服务）。
//! - 环境变量备份/恢复（用户 + 系统）；Uninstall 注册表只读展示。
//! - 软件描述：预置映射 + AI fallback。
//! - 可逆性：不提供一键还原，但记完整迁移历史。

pub mod actions;
pub mod baseline;
pub mod describe;
pub mod envvar;
pub mod identify;
pub mod migrate;
pub mod models;
pub mod monitor;
pub mod tray;
pub mod uninstall;

use async_trait::async_trait;
use serde_json::Value;

use crate::app::context::Context;
use crate::app::plugin::{
    FeatureType, Plugin, PluginFeature, PluginMetadata, PluginPermissions, PluginType,
};

pub struct AppMoverPlugin {
    meta: PluginMetadata,
}

impl AppMoverPlugin {
    pub fn new() -> Self {
        Self {
            meta: PluginMetadata {
                id: "appmover".into(),
                name: "软件迁移".into(),
                icon: "HardDriveDownload".into(),
                plugin_type: PluginType::Ui,
                features: vec![
                    PluginFeature {
                        code: "migrate".into(),
                        explain: "目录迁移".into(),
                        cmds: vec!["迁移".into(), "migrate".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/am/migrate".into()),
                    },
                    PluginFeature {
                        code: "monitor".into(),
                        explain: "目录监控".into(),
                        cmds: vec!["监控".into(), "monitor".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/am/monitor".into()),
                    },
                    PluginFeature {
                        code: "envvar".into(),
                        explain: "环境变量".into(),
                        cmds: vec!["环境变量".into(), "envvar".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/am/envvar".into()),
                    },
                    PluginFeature {
                        code: "baseline".into(),
                        explain: "基线管理".into(),
                        cmds: vec!["基线".into(), "baseline".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/am/baseline".into()),
                    },
                    PluginFeature {
                        code: "history".into(),
                        explain: "迁移历史".into(),
                        cmds: vec!["历史".into(), "history".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/am/history".into()),
                    },
                ],
                version: env!("CARGO_PKG_VERSION").into(),
                permissions: PluginPermissions::all(),
            },
        }
    }
}

impl Default for AppMoverPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for AppMoverPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.meta
    }

    fn actions(&self) -> Vec<&'static str> {
        vec![
            // 识别
            "am:scan_candidates",
            "am:describe",
            "am:describe_update",
            "am:list_describe",
            // 基线 / 保护集
            "am:import_baseline",
            "am:set_first_scan_as_baseline",
            "am:get_protected",
            "am:add_protected",
            "am:remove_protected",
            // 目标根映射
            "am:get_target_map",
            "am:set_target_map",
            "am:remove_target_map",
            // 迁移
            "am:plan_migration",
            "am:scan_locks",
            "am:kill_locks",
            "am:execute_migration",
            "am:retry_migration",
            "am:list_jobs",
            // 监控
            "am:start_monitor",
            "am:stop_monitor",
            "am:get_monitor_events",
            "am:dismiss_event",
            // 环境变量 / 卸载表
            "am:backup_env",
            "am:restore_env",
            "am:list_env_backups",
            "am:list_installed",
            // 托盘 / 自启
            "am:get_badge",
            "am:refresh_badge",
            "am:get_autostart",
            "am:set_autostart",
        ]
    }

    async fn invoke(
        &self,
        action: &str,
        args: Value,
        ctx: &Context,
    ) -> Result<Value, crate::app::plugin::PluginError> {
        actions::dispatch(action, args, ctx).await
    }
}
