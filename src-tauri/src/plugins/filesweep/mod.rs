pub mod actions;

use async_trait::async_trait;
use serde_json::Value;

use crate::app::context::Context;
use crate::app::plugin::{
    FeatureType, Plugin, PluginFeature, PluginMetadata, PluginPermissions, PluginType,
};

pub struct FileSweepPlugin {
    meta: PluginMetadata,
}

impl FileSweepPlugin {
    pub fn new() -> Self {
        Self {
            meta: PluginMetadata {
                id: "filesweep".into(),
                name: "文件整理".into(),
                icon: "Folder".into(),
                plugin_type: PluginType::Ui,
                features: vec![
                    PluginFeature {
                        code: "files".into(),
                        explain: "全部文件".into(),
                        cmds: vec!["文件".into(), "files".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/files".into()),
                    },
                    PluginFeature {
                        code: "scan".into(),
                        explain: "扫描文件".into(),
                        cmds: vec!["扫描".into(), "scan".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/scan".into()),
                    },
                    PluginFeature {
                        code: "dedup".into(),
                        explain: "重复文件".into(),
                        cmds: vec!["去重".into(), "重复".into(), "dedup".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/files".into()),
                    },
                    PluginFeature {
                        code: "catalog".into(),
                        explain: "软件目录".into(),
                        cmds: vec!["目录".into(), "catalog".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/catalog".into()),
                    },
                    PluginFeature {
                        code: "enrich".into(),
                        explain: "AI 丰富".into(),
                        cmds: vec!["AI".into(), "丰富".into(), "enrich".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/enrich".into()),
                    },
                ],
                version: env!("CARGO_PKG_VERSION").into(),
                permissions: PluginPermissions::all(),
            },
        }
    }
}

impl Default for FileSweepPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for FileSweepPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.meta
    }

    fn actions(&self) -> Vec<&'static str> {
        vec![
            "scan:start",
            "scan:cancel",
            "scan:files",
            "scan:stats",
            "scan:suggestions",
            "scan:suggestions_v2",
            "clean:start",
            "catalog:get",
            "catalog:update",
            "catalog:delete",
            "catalog:export",
            "enrich:start",
            "enrich:status",
            "settings:get",
            "settings:update",
            "rules:get",
            "rules:update",
            "categories:get",
            "categories:update",
            "tags:get",
            "tags:create",
            "tags:update",
            "tags:delete",
            "files:set_action",
            "files:set_move_target",
            "files:batch_set_action",
            "logs:get",
            "logs:revert",
            "logs:batch_revert",
            "db:reset",
            // ── 配置 DB 化（config:*）──
            "config:roots:list",
            "config:roots:add",
            "config:roots:update",
            "config:roots:delete",
            "config:categories:list",
            "config:categories:add",
            "config:categories:update",
            "config:categories:delete",
            "config:func_categories:list",
            "config:func_categories:add",
            "config:func_categories:update",
            "config:func_categories:delete",
            "config:exclude:list",
            "config:exclude:add",
            "config:exclude:update",
            "config:exclude:delete",
            "config:tags:list",
            "config:tags:add",
            "config:tags:update",
            "config:tags:delete",
        ]
    }

    async fn invoke(&self, action: &str, args: Value, ctx: &Context) -> Result<Value, crate::app::plugin::PluginError> {
        actions::dispatch(action, args, ctx).await
    }
}
