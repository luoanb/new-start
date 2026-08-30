//! 知识演化领域服务：creator 变体状态机（观察→晋升→淘汰→差分重写）。
//!
//! 依赖方向：`Evolution → Selection`（复用 `ensure_creator`）；模型出网走
//! `model_caller`，JSON 解析复用 `model.rs` 共享函数。
use std::sync::{Arc, Mutex};

use crate::core::{
    error::{AppError, AppResult},
    insert_catalog::InsertCatalog,
    log_phase::{
        PHASE_NEURON_EVOLVE_CREATOR, PHASE_NEURON_REWRITE_VARIANT, PHASE_NEURON_ROLLBACK_VARIANT,
    },
    model_call_input::{ModelAppendTemplate, ModelCallInput},
    models::{Neuron, NeuronUpdate, NeuronVariant},
    neuron::{
        model::{extract_json_object, NeuronModelCaller},
        selection::NeuronSelection,
        store::NeuronStore,
    },
};

use super::lock_error;

pub(crate) struct NeuronEvolution {
    store: Arc<Mutex<NeuronStore>>,
    model_caller: Arc<dyn NeuronModelCaller>,
    selection: Arc<NeuronSelection>,
}

impl NeuronEvolution {
    pub(crate) fn new(
        store: Arc<Mutex<NeuronStore>>,
        model_caller: Arc<dyn NeuronModelCaller>,
        selection: Arc<NeuronSelection>,
    ) -> Self {
        Self {
            store,
            model_caller,
            selection,
        }
    }

    fn store(&self) -> AppResult<std::sync::MutexGuard<'_, NeuronStore>> {
        self.store.lock().map_err(lock_error)
    }

    /// Bump `use_count` / `last_used_at` for a variant that was just used to
    /// generate a child neuron.
    pub(crate) fn record_variant_usage(&self, variant_id: &str) -> AppResult<Neuron> {
        self.store()?.increment_variant_usage(variant_id)
    }

    /// Accumulate a signed score delta onto a variant (lineage attribution).
    pub(crate) fn accumulate_variant_delta(
        &self,
        variant_id: &str,
        delta: f64,
    ) -> AppResult<Neuron> {
        self.store()?.accumulate_variant_delta(variant_id, delta)
    }

    /// Evaluate the creator variant pool after a score feedback round.
    /// Steps (each call acts on at most ONE variant):
    /// 1. Observing slots: promote when `use_count >= 1`, rollback when delta < 0.
    /// 2. Elimination candidates (delta <= -3, or use_count >= 10 with delta < 0).
    /// 3. Rewrite candidates (use_count >= 3 and |delta| >= 2): differential rewrite.
    pub(crate) async fn maybe_evolve_creator_variants(&self) -> AppResult<()> {
        tracing::info!(phase = PHASE_NEURON_EVOLVE_CREATOR, "entry: ensure_creator");
        let creator = self.selection.ensure_creator()?;
        let variants = self.store()?.get_variants(&creator.id, false)?;
        tracing::info!(
            phase = PHASE_NEURON_EVOLVE_CREATOR,
            creator_id = %creator.id,
            variant_count = variants.len(),
            "variants loaded"
        );
        if variants.is_empty() {
            return Ok(());
        }

        // 1. Observing slots.
        for variant in variants
            .iter()
            .filter(|v| v.variant_state.as_deref() == Some("observing"))
        {
            if variant.accumulated_delta < 0.0 {
                self.rollback_variant_if_regressed(&variant.neuron.id)?;
                return Ok(());
            }
            if variant.use_count >= 1 {
                self.store()?
                    .set_variant_state(&variant.neuron.id, Some("active"))?;
                tracing::info!(
                    phase = PHASE_NEURON_EVOLVE_CREATOR,
                    variant_id = %variant.neuron.id,
                    use_count = variant.use_count,
                    "observing variant promoted to active"
                );
                return Ok(());
            }
        }

        // 2. Elimination candidates.
        for variant in variants
            .iter()
            .filter(|v| v.variant_state.as_deref() != Some("observing"))
        {
            if variant.manual_edited {
                continue;
            }
            let eliminated = variant.accumulated_delta <= -3.0
                || (variant.use_count >= 10 && variant.accumulated_delta < 0.0);
            if eliminated {
                self.rollback_variant_if_regressed(&variant.neuron.id)?;
                tracing::info!(
                    phase = PHASE_NEURON_EVOLVE_CREATOR,
                    variant_id = %variant.neuron.id,
                    accumulated_delta = variant.accumulated_delta,
                    use_count = variant.use_count,
                    "variant eliminated; rolling back"
                );
                return Ok(());
            }
        }

        // 3. Rewrite candidate (at most one per call).
        for variant in variants
            .iter()
            .filter(|v| v.variant_state.as_deref() != Some("observing"))
        {
            if variant.manual_edited {
                continue;
            }
            if variant.use_count >= 3 && variant.accumulated_delta.abs() >= 2.0 {
                match self.rewrite_variant(&creator, variant).await {
                    Ok(()) => {
                        tracing::info!(
                            phase = PHASE_NEURON_EVOLVE_CREATOR,
                            variant_id = %variant.neuron.id,
                            "variant differentially rewritten; moved to observing"
                        );
                        return Ok(());
                    }
                    Err(error) => {
                        // Failure keeps the old version; never blocks the create flow.
                        tracing::warn!(
                            phase = PHASE_NEURON_EVOLVE_CREATOR,
                            variant_id = %variant.neuron.id,
                            error = %error,
                            "rewrite failed; keeping old version"
                        );
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    /// Restore a variant to its most recent archived version.
    /// With no history, demote the variant to observing so it exits the active pool.
    fn rollback_variant_if_regressed(&self, variant_id: &str) -> AppResult<()> {
        let store = self.store()?;
        let Some(version) = store.latest_version_of(variant_id)? else {
            store.set_variant_state(variant_id, Some("observing"))?;
            tracing::info!(
                phase = PHASE_NEURON_ROLLBACK_VARIANT,
                variant_id,
                "no archived version; demoted to observing"
            );
            return Ok(());
        };
        store.update_neuron(
            variant_id,
            NeuronUpdate {
                desc: None,
                content: Some(version.content.clone()),
                ..Default::default()
            },
        )?;
        store.insert_neuron_version(variant_id, &version.content, "rollback", Some(&version.id))?;
        store.set_variant_state(variant_id, Some("active"))?;
        tracing::info!(
            phase = PHASE_NEURON_ROLLBACK_VARIANT,
            variant_id,
            version_id = %version.id,
            "rolled back to archived version"
        );
        Ok(())
    }

    /// Differential rewrite of one variant via the `creator.variant_evolve` contract.
    /// On success the variant is updated with the new content and moved to the
    /// observing slot; the previous content is archived in `neuron_versions`.
    async fn rewrite_variant(&self, creator: &Neuron, variant: &NeuronVariant) -> AppResult<()> {
        tracing::info!(
            phase = PHASE_NEURON_REWRITE_VARIANT,
            variant_id = %variant.neuron.id,
            use_count = variant.use_count,
            accumulated_delta = variant.accumulated_delta,
            "rewrite variant entry"
        );
        let payload = serde_json::json!({
            "current_desc": variant.neuron.desc,
            "current_content": variant.neuron.content,
            "current_tool_ids": variant.neuron.tool_ids,
            "use_count": variant.use_count,
            "accumulated_delta": variant.accumulated_delta,
            "last_used_at": variant.last_used_at,
            "parent_creator_content": creator.content,
        });
        let insert = InsertCatalog::require("creator.variant_evolve");
        let wire = ModelCallInput::assemble(
            &[],
            &creator.content,
            insert,
            &payload.to_string(),
            ModelAppendTemplate::Manual,
        );
        let output = self.model_caller.call_model(wire).await?;
        let decision = extract_json_object(&output)?;
        let desc = decision
            .get("desc")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| variant.neuron.desc.clone());
        let content = decision
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::InvalidInput("rewrite response missing content".into()))?;

        let store = self.store()?;
        // Archive the current version before mutating.
        let prev = store.latest_version_of(&variant.neuron.id)?;
        store.insert_neuron_version(
            &variant.neuron.id,
            &variant.neuron.content,
            "evolve",
            prev.as_ref().map(|p| p.id.as_str()),
        )?;
        store.update_neuron(
            &variant.neuron.id,
            NeuronUpdate {
                desc: Some(desc),
                content: Some(content.to_string()),
                ..Default::default()
            },
        )?;
        store.set_variant_state(&variant.neuron.id, Some("observing"))?;
        Ok(())
    }
}
