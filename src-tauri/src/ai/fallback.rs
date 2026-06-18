use std::future::Future;
use std::pin::Pin;

use crate::ai::enricher::{default_enrich_result, EnrichRequest, EnrichResult, Enricher};

// ────────────────── FallbackEnricher ──────────────────

/// 双层 fallback 补全器：先尝试 primary，若置信度不足（<0.5）或描述为空，
/// 则尝试 secondary，最终返回置信度更高的结果。
pub struct FallbackEnricher {
    primary: Box<dyn Enricher>,
    secondary: Box<dyn Enricher>,
}

impl FallbackEnricher {
    pub fn new(
        primary: Option<Box<dyn Enricher>>,
        secondary: Option<Box<dyn Enricher>>,
    ) -> Self {
        Self {
            primary: primary.unwrap_or_else(|| {
                Box::new(crate::ai::offline::OfflineEnricher::new(""))
            }),
            secondary: secondary.unwrap_or_else(|| {
                Box::new(crate::ai::offline::OfflineEnricher::new(""))
            }),
        }
    }
}

impl Enricher for FallbackEnricher {
    fn enrich(
        &self,
        req: EnrichRequest,
        categories: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<EnrichResult, Box<dyn std::error::Error + Send + Sync>>> + Send + '_>>
    {
        // 因为 primary/secondary 是 &Box<dyn Enricher>，
        // async block 中可以安全地引用它们（lifetime tied to &self）。
        let primary = &self.primary;
        let secondary = &self.secondary;

        Box::pin(async move {
            let req_name = req.name.clone();
            let primary_result = primary
                .enrich(req.clone(), categories.clone())
                .await
                .unwrap_or_else(|e| {
                    log::warn!(
                        "FallbackEnricher primary '{}' failed: {}",
                        req_name,
                        e
                    );
                    default_enrich_result(primary.name())
                });

            // 若 primary 结果足够好，直接返回
            if primary_result.confidence >= 0.5 && !primary_result.description.is_empty() {
                return Ok(primary_result);
            }

            // primary 不够好，尝试 secondary
            let req_name2 = req.name.clone();
            let secondary_result = secondary
                .enrich(req, categories)
                .await
                .unwrap_or_else(|e| {
                    log::warn!(
                        "FallbackEnricher secondary '{}' failed: {}",
                        req_name2,
                        e
                    );
                    default_enrich_result(secondary.name())
                });

            // 返回置信度更高的结果；相等时优先 primary
            if secondary_result.confidence > primary_result.confidence {
                Ok(secondary_result)
            } else {
                Ok(primary_result)
            }
        })
    }

    fn name(&self) -> &str {
        "fallback"
    }
}
