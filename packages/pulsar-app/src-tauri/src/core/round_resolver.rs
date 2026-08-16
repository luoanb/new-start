//! ① 选型决策 + 角色上下文拼接（v2）：resolve 目标单一——获取角色神经元。
//!
//! 原 `NeuronCallService::resolve_role` 迁入。纯决策（可含 LLM 选型），不接触会话/落库；
//! 选中的神经元 context 按规则拼到 old_messages（首轮 → System，后续轮 → RoleContext）。
//! 工具授权不归本组件（按会话模式，落点在 `round_executor`）；输入消息构造/落库归 runner。

use std::sync::Arc;

use super::{
    conversation_store::now_ms,
    error::{AppError, AppResult},
    insert_catalog::InsertCatalog,
    model_call_input::ModelCallInput,
    models::{
        AssistantCandidateScope, Message, MessageBody, MessageRole, NeighborhoodPoolPolicy, Neuron,
        SelectionPolicy, SessionBehavior, ToolPolicy, DEFAULT_ASSISTANT_GLOBAL_LIMIT,
    },
    neuron_manager::NeuronManager,
    round_types::SessionSeed,
};

/// 选型决策组件：只做「本轮选谁、把角色上下文拼进 messages」，不做模型调用/工具执行。
#[derive(Debug)]
pub struct RoundResolver {
    neuron_manager: Arc<NeuronManager>,
}

impl RoundResolver {
    pub fn new(neuron_manager: Arc<NeuronManager>) -> Self {
        Self { neuron_manager }
    }

    /// 选型 + 角色上下文拼接，目标单一：获取角色神经元。
    ///
    /// 输入：`seed`（选型起点）、`last_selected`（上一轮锚点，降频复用）、`old_messages`
    /// （落库历史真相源）、`reselect`（是否本轮重新选型；false 且有锚点 → 沿用锚点）。
    ///
    /// 输出：`(new_messages, selected_neuron)`——`new_messages` = old + 角色上下文
    /// （选中神经元且首轮 → `System(neuron.content)`；非首轮 → `RoleContext("[当前角色]\n" + content)`；
    /// 未选中 → 原样返回 old），不含本轮输入消息（由 runner 构造追加）。
    ///
    /// - `None`（直连）：不选型、无角色上下文。
    /// - `Global`：无历史全域池选 1；有锚点且复用轮沿用；LLM 选型后推进。
    /// - `Neuron(普通)`：默认邻域行为（锚点 = 自身）。
    /// - `Neuron(系统)`：用 behavior（`None` 不选型 / `Fixed` 读自己 content /
    ///   `Neighborhood` 锚点规则；`Global` 禁用于系统神经元 → 宽容回退 Neighborhood）。
    pub async fn resolve(
        &self,
        seed: Option<&SessionSeed>,
        last_selected: Option<&str>,
        old_messages: &[Message],
        reselect: bool,
    ) -> AppResult<(Vec<Message>, Option<Neuron>)> {
        let Some(seed) = seed else {
            // 直连（Chat 等）：不选型、无角色上下文，原样返回历史。
            return Ok((old_messages.to_vec(), None));
        };
        let neuron = match seed {
            SessionSeed::Global => {
                let behavior = SessionBehavior {
                    selection: SelectionPolicy::Global {
                        limit: DEFAULT_ASSISTANT_GLOBAL_LIMIT,
                    },
                    tools: ToolPolicy::None,
                    insert_id: None,
                };
                let scope = Self::scope_for_selection(
                    &behavior.selection,
                    "",
                    last_selected,
                )
                .expect("Global always produce a scope");
                if let Some(role) = self.reuse_selected_neuron(last_selected, reselect) {
                    tracing::info!(
                        phase = "resolve_role",
                        seed = ?SessionSeed::Global,
                        reused_anchor = true,
                        neuron_id = %role.id,
                        "reusing last-selected neuron (no LLM selection)"
                    );
                    Some(role)
                } else {
                    let role = self
                        .neuron_manager
                        .select_role(&ModelCallInput::project_history(old_messages), scope)
                        .await?;
                    tracing::info!(
                        phase = "resolve_role",
                        seed = ?SessionSeed::Global,
                        reused_anchor = false,
                        neuron_id = %role.id,
                        "LLM-selected neuron (global pool)"
                    );
                    Some(role)
                }
            }
            SessionSeed::Neuron(id) => {
                let neuron = self
                    .neuron_manager
                    .get(id)?
                    .ok_or_else(|| AppError::NeuronNotFound(id.clone()))?;
                if neuron.system_type.is_none() {
                    // 普通神经元：推导默认领域行为（邻域锚点 = 自身）。
                    let behavior = SessionBehavior {
                        selection: SelectionPolicy::Neighborhood {
                            policy: NeighborhoodPoolPolicy::default(),
                        },
                        tools: ToolPolicy::FromNeuron,
                        insert_id: None,
                    };
                    let scope = Self::scope_for_selection(&behavior.selection, id, last_selected)
                        .expect("Neighborhood always produce a scope");
                    if let Some(role) = self.reuse_selected_neuron(last_selected, reselect) {
                        tracing::info!(
                            phase = "resolve_role",
                            seed = ?SessionSeed::Neuron(id.clone()),
                            reused_anchor = true,
                            neuron_id = %role.id,
                            "reusing last-selected neuron (no LLM selection)"
                        );
                        Some(role)
                    } else {
                        let role = self
                            .neuron_manager
                            .select_role(&ModelCallInput::project_history(old_messages), scope)
                            .await?;
                        tracing::info!(
                            phase = "resolve_role",
                            seed = ?SessionSeed::Neuron(id.clone()),
                            reused_anchor = false,
                            neuron_id = %role.id,
                            "LLM-selected neuron (neighborhood)"
                        );
                        Some(role)
                    }
                } else {
                    // 系统神经元：用 behavior（禁 Global，旧数据宽容回退 Neighborhood）。
                    let behavior = neuron.behavior.clone().ok_or_else(|| {
                        AppError::InvalidInput(format!(
                            "neuron {id} is a system neuron but has no behavior"
                        ))
                    })?;
                    let selection = match &behavior.selection {
                        SelectionPolicy::Global { .. } => SelectionPolicy::Neighborhood {
                            policy: NeighborhoodPoolPolicy::default(),
                        },
                        other => other.clone(),
                    };
                    match &selection {
                        SelectionPolicy::None => {
                            tracing::info!(
                                phase = "resolve_role",
                                seed = ?SessionSeed::Neuron(id.clone()),
                                selection = "none",
                                "system neuron: no selection"
                            );
                            None
                        }
                        SelectionPolicy::Fixed => {
                            // 读系统神经元自己的 content；不参与 LLM 选型。
                            tracing::info!(
                                phase = "resolve_role",
                                seed = ?SessionSeed::Neuron(id.clone()),
                                selection = "fixed",
                                neuron_id = %neuron.id,
                                "system neuron: fixed role, no LLM selection"
                            );
                            Some(neuron)
                        }
                        SelectionPolicy::Neighborhood { .. } => {
                            let scope = Self::scope_for_selection(&selection, id, last_selected)
                                .expect("Neighborhood always produce a scope");
                            if let Some(role) = self.reuse_selected_neuron(last_selected, reselect)
                            {
                                tracing::info!(
                                    phase = "resolve_role",
                                    seed = ?SessionSeed::Neuron(id.clone()),
                                    selection = "neighborhood",
                                    reused_anchor = true,
                                    neuron_id = %role.id,
                                    "reusing last-selected neuron (no LLM selection)"
                                );
                                Some(role)
                            } else {
                                let role = self
                                    .neuron_manager
                                    .select_role(
                                        &ModelCallInput::project_history(old_messages),
                                        scope,
                                    )
                                    .await?;
                                tracing::info!(
                                    phase = "resolve_role",
                                    seed = ?SessionSeed::Neuron(id.clone()),
                                    selection = "neighborhood",
                                    reused_anchor = false,
                                    neuron_id = %role.id,
                                    "LLM-selected neuron (neighborhood)"
                                );
                                Some(role)
                            }
                        }
                        SelectionPolicy::Global { .. } => {
                            unreachable!("converted to Neighborhood above")
                        }
                    }
                }
            }
        };
        let messages = Self::attach_role(old_messages, neuron.as_ref());
        Ok((messages, neuron))
    }

    /// 角色上下文拼接（v2）：选中神经元时——首轮（old 为空）追加 `System(neuron.content)`，
    /// 若神经元带 `behavior.insert_id` 再追加**独立第二条 System 消息**（契约段，形态 B/D4a；
    /// v1 由 `assemble_with_context` 注入，随重构误删，裁决模型需此硬契约如 action/scope_in）；
    /// 后续轮追加 `RoleContext("[当前角色]\n" + content)`（契约段不重复，历史回灌自带）；
    /// 未选中神经元原样返回 old。输入消息不在此拼接（runner 追加）。
    ///
    /// 契约段拼接（D4a 方案 1，不改追加流程）：带 `behavior.insert_id` 时契约段并入角色内容
    /// `format!("{}\n\n{}", 角色内容, 契约段)`；注入条件 = `old_messages` 无消息内容包含该契约段
    /// （主对话首轮拼、后续轮历史回灌自带不重复；裁决调用历史无契约 → 每轮拼）。
    fn attach_role(old_messages: &[Message], neuron: Option<&Neuron>) -> Vec<Message> {
        let Some(neuron) = neuron else {
            return old_messages.to_vec();
        };
        if neuron.content.trim().is_empty() {
            return old_messages.to_vec();
        }
        let contract = neuron
            .behavior
            .as_ref()
            .and_then(|b| b.insert_id.as_deref())
            .map(InsertCatalog::require);
        let already_has_contract = contract.map_or(false, |c| {
            old_messages.iter().any(|m| match &m.body {
                MessageBody::Text { content } | MessageBody::RoleContext { content } => {
                    content.contains(c)
                }
                _ => false,
            })
        });
        let mut out = old_messages.to_vec();
        if out.is_empty() {
            // 首轮：角色进 System（历史为空，落库即第一条，后续轮天然稳定）。
            out.push(Message {
                role: MessageRole::System,
                body: MessageBody::Text {
                    content: Self::join_contract(&neuron.content, contract, already_has_contract),
                },
                timestamp: now_ms(),
                neuron_id: None,
            });
        } else {
            out.push(Message {
                role: MessageRole::User,
                body: MessageBody::RoleContext {
                    content: Self::join_contract(
                        &format!("[当前角色]\n{}", neuron.content),
                        contract,
                        already_has_contract,
                    ),
                },
                timestamp: now_ms(),
                neuron_id: Some(neuron.id.clone()),
            });
        }
        out
    }

    /// 角色内容拼接契约段（方案 1）：有契约段且历史未含 → `"{role}\n\n{contract}"`，否则原样。
    fn join_contract(role: &str, contract: Option<&str>, already_has: bool) -> String {
        match (contract, already_has) {
            (Some(c), false) => format!("{}\n\n{}", role, c),
            _ => role.to_string(),
        }
    }

    /// 选型降频（复用轮）：`reselect == false` 且有历史锚点时，直接沿用
    /// `last_selected_neuron_id` 作为本轮角色（跳过 LLM 选型）。
    /// `true`（选型轮）/ 锚点缺失返回 `None`，走正常选型。
    fn reuse_selected_neuron(&self, last_selected: Option<&str>, reselect: bool) -> Option<Neuron> {
        if reselect {
            return None;
        }
        let id = last_selected?;
        self.neuron_manager.get(id).ok().flatten()
    }

    /// selection → 候选池装配 scope（委托 NeuronManager，`resolve` 共用语义）。
    fn scope_for_selection(
        selection: &SelectionPolicy,
        spec_neuron_id: &str,
        last_selected: Option<&str>,
    ) -> Option<AssistantCandidateScope> {
        NeuronManager::scope_for_selection(selection, spec_neuron_id, last_selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::MessageRole;

    fn neuron_with_insert(insert_id: Option<&str>) -> Neuron {
        Neuron {
            id: "n_test".into(),
            desc: "test neuron".into(),
            content: "ROLE CONTENT".into(),
            weight: 0.0,
            system_type: Some("assistant_test".into()),
            tool_ids: vec![],
            created_at: 0,
            updated_at: 0,
            use_count: 0,
            last_used_at: None,
            deleted_at: None,
            behavior: Some(SessionBehavior {
                insert_id: insert_id.map(|s| s.to_string()),
                ..Default::default()
            }),
        }
    }

    fn contract_text() -> &'static str {
        InsertCatalog::require("assistant.match_topic")
    }

    #[test]
    fn attach_role_first_round_with_insert_merges_contract_into_system() {
        // 首轮 + 带 insert_id（方案 1）：单条 System，内容 = 角色 + "\n\n" + 契约段。
        let out = RoundResolver::attach_role(&[], Some(&neuron_with_insert(Some("assistant.match_topic"))));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, MessageRole::System);
        assert_eq!(
            out[0].body,
            MessageBody::Text {
                content: format!("ROLE CONTENT\n\n{}", contract_text()),
            }
        );
    }

    #[test]
    fn attach_role_first_round_without_insert_appends_single_system() {
        // 首轮 + 无 insert_id（普通神经元 / behavior=None）：仅 System(ROLE)，无契约段。
        let out = RoundResolver::attach_role(&[], Some(&neuron_with_insert(None)));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, MessageRole::System);
        assert_eq!(out[0].body, MessageBody::Text { content: "ROLE CONTENT".into() });
    }

    #[test]
    fn attach_role_later_round_without_contract_in_history_merges_into_role_context() {
        // 非首轮 + 带 insert_id + 历史无契约段（裁决调用场景）：RoleContext 内容拼契约段。
        let history = vec![Message {
            role: MessageRole::User,
            body: MessageBody::Text { content: "用户输入".into() },
            timestamp: 0,
            neuron_id: None,
        }];
        let out = RoundResolver::attach_role(&history, Some(&neuron_with_insert(Some("assistant.match_topic"))));
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].role, MessageRole::User);
        assert_eq!(
            out[1].body,
            MessageBody::RoleContext {
                content: format!("[当前角色]\nROLE CONTENT\n\n{}", contract_text()),
            }
        );
    }

    #[test]
    fn attach_role_later_round_with_contract_in_history_skips_merge() {
        // 非首轮 + 带 insert_id + 历史已含契约段（主对话后续轮，首轮落库自带）：不重复拼接。
        let history = vec![Message {
            role: MessageRole::System,
            body: MessageBody::Text {
                content: format!("prev system\n\n{}", contract_text()),
            },
            timestamp: 0,
            neuron_id: None,
        }];
        let out = RoundResolver::attach_role(&history, Some(&neuron_with_insert(Some("assistant.match_topic"))));
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[1].body,
            MessageBody::RoleContext {
                content: "[当前角色]\nROLE CONTENT".into(),
            }
        );
    }

    #[test]
    fn attach_role_unselected_returns_old_untouched() {
        let history = vec![Message {
            role: MessageRole::User,
            body: MessageBody::Text { content: "hi".into() },
            timestamp: 0,
            neuron_id: None,
        }];
        let out = RoundResolver::attach_role(&history, None);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].body, MessageBody::Text { content: "hi".into() });
    }
}
