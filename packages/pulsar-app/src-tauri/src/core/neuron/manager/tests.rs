    use std::{
        collections::HashSet,
        fs,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex, RwLock,
        },
    };

    use async_trait::async_trait;
    use rusqlite::Connection as SqliteConnection;

    use super::*;
    use crate::core::{
        error::{AppError, AppResult},
        models::{
            AssistantCandidateScope, CandidateQuery, CreateNeuronInput, EnsureSystemOpts,
            ModelMessage, ModelMessageRole, NeighborhoodPoolPolicy, Neuron, NeuronCreate,
            NeuronKindFilter, NeuronUpdate, SelectionPolicy, SessionBehavior, ToolPolicy,
        },
        neuron::{
            config::{NeuronConfigReader, SYSTEM_PROMPT_SEEDS},
            model::NeuronModelCaller,
            store::NeuronStore,
        },
        tool_registry::ToolRegistry,
    };

    struct MockModelCaller {
        calls: AtomicUsize,
    }

    fn prompt_blob_from_messages(messages: &[ModelMessage]) -> String {
        messages
            .iter()
            .filter(|m| matches!(m.role, ModelMessageRole::System | ModelMessageRole::User))
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[async_trait]
    impl NeuronModelCaller for MockModelCaller {
        async fn call_model(&self, messages: Vec<ModelMessage>) -> AppResult<String> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            let user_prompt = prompt_blob_from_messages(&messages);
            let count = user_prompt
                .split("exactly ")
                .nth(1)
                .and_then(|rest| {
                    rest.split_whitespace().next().and_then(|token| {
                        token
                            .trim_matches(|c: char| !c.is_ascii_digit())
                            .parse()
                            .ok()
                    })
                })
                .unwrap_or(1usize);
            if count <= 1 {
                return Ok(format!(
                    r#"{{"desc":"generated-{call}","content":"content-{call}","weight":1.0,"tool_ids":[]}}"#
                ));
            }
            let items: Vec<String> = (0..count)
                .map(|i| {
                    format!(
                        r#"{{"desc":"generated-{call}-{i}","content":"content-{call}-{i}","weight":1.0,"tool_ids":[]}}"#
                    )
                })
                .collect();
            Ok(format!("[{}]", items.join(",")))
        }
    }

    /// 固定返回指定 `neuron_id` 的选型 mock：驱动 LLM 命中指定候选（供回挂边用例使用）。
    struct PickIdModelCaller {
        target_id: Mutex<String>,
    }

    #[async_trait]
    impl NeuronModelCaller for PickIdModelCaller {
        async fn call_model(&self, _messages: Vec<ModelMessage>) -> AppResult<String> {
            let id = self.target_id.lock().unwrap().clone();
            Ok(format!(r#"{{"neuron_id":"{id}"}}"#))
        }
    }

    fn test_manager() -> (Arc<NeuronManager>, std::path::PathBuf) {
        test_manager_with(Arc::new(MockModelCaller {
            calls: AtomicUsize::new(0),
        }))
    }

    fn test_manager_with(
        caller: Arc<dyn NeuronModelCaller>,
    ) -> (Arc<NeuronManager>, std::path::PathBuf) {
        let conn = Arc::new(Mutex::new(SqliteConnection::open_in_memory().unwrap()));
        let store = Arc::new(Mutex::new(NeuronStore::new(conn)));
        store.lock().unwrap().init_table().unwrap();
        let root = std::env::temp_dir().join(format!(
            "pulsar-neuron-manager-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("config.json"),
            r#"{"neurons":{"bootstrap":{"create_neuron_prompt":"create a neuron"}}}"#,
        )
        .unwrap();
        let manager = Arc::new(NeuronManager::new(
            store,
            caller,
            NeuronConfigReader::new(root.clone()),
            Arc::new(RwLock::new(ToolRegistry::new())),
        ));
        (manager, root)
    }

    fn insert_plain(manager: &NeuronManager, desc: &str, content: &str) -> Neuron {
        manager
            .store()
            .unwrap()
            .create_neuron(NeuronCreate {
                desc: desc.into(),
                content: content.into(),
                ..Default::default()
            })
            .unwrap()
    }

    fn insert_downstream(manager: &NeuronManager, parent_id: &str, desc: &str) -> Neuron {
        manager
            .create_plain(
                NeuronCreate {
                    desc: desc.into(),
                    content: format!("{desc} content"),
                    ..Default::default()
                },
                Some(parent_id),
            )
            .unwrap()
    }

    /// 直接落库选型系统神经元（无下游填充）：驱动 `try_llm_select` 走 LLM 成功分支。
    fn insert_selector(manager: &NeuronManager) -> Neuron {
        manager
            .store()
            .unwrap()
            .create_neuron(NeuronCreate {
                desc: "selector".into(),
                content: "select one".into(),
                weight: 0.0,
                system_type: Some(ASSISTANT_SELECT_NEURON.into()),
                tool_ids: vec![],
                lineage_parent_id: None,
                variant_state: None,
            })
            .unwrap()
    }

    #[tokio::test]
    async fn select_candidates_prefers_source_id_and_fills_to_n() {
        let (manager, root) = test_manager();
        let source = insert_plain(&manager, "source", "source content");
        let candidates = manager
            .select_candidates(CandidateQuery {
                n: 3,
                source_id: Some(source.id.clone()),
                min_new: 2,
            })
            .await
            .unwrap();
        assert_eq!(candidates.len(), 3);
        let downstream = manager
            .store()
            .unwrap()
            .list_direct_downstream(&source.id, 10, &HashSet::new())
            .unwrap();
        assert_eq!(downstream.len(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_builds_children_self_siblings_and_three_ancestors() {
        let (manager, root) = test_manager();
        let great_grandparent = insert_plain(&manager, "great-grandparent", "great");
        let grandparent = insert_downstream(&manager, &great_grandparent.id, "grandparent");
        let parent = insert_downstream(&manager, &grandparent.id, "parent");
        let self_neuron = insert_downstream(&manager, &parent.id, "self");
        let sibling = insert_downstream(&manager, &parent.id, "sibling");
        let existing_child_a = insert_downstream(&manager, &self_neuron.id, "child-a");
        let existing_child_b = insert_downstream(&manager, &self_neuron.id, "child-b");

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::neighborhood_default(
                self_neuron.id.clone(),
            ))
            .await
            .unwrap();
        let candidate_ids: HashSet<&str> = candidates.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(candidate_ids.len(), candidates.len());
        assert!(candidate_ids.contains(self_neuron.id.as_str()));
        assert!(candidate_ids.contains(sibling.id.as_str()));
        assert!(candidate_ids.contains(parent.id.as_str()));
        assert!(candidate_ids.contains(grandparent.id.as_str()));
        assert!(candidate_ids.contains(great_grandparent.id.as_str()));
        assert!(candidate_ids.contains(existing_child_a.id.as_str()));
        assert!(candidate_ids.contains(existing_child_b.id.as_str()));

        let downstream = manager
            .store()
            .unwrap()
            .list_direct_downstream(&self_neuron.id, 20, &HashSet::new())
            .unwrap();
        assert_eq!(downstream.len(), 6);
        assert_eq!(candidates.len(), 11);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_adds_two_new_children_when_four_exist() {
        let (manager, root) = test_manager();
        let self_neuron = insert_plain(&manager, "self", "self");
        for index in 0..4 {
            insert_downstream(&manager, &self_neuron.id, &format!("child-{index}"));
        }

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::neighborhood_default(
                self_neuron.id.clone(),
            ))
            .await
            .unwrap();
        let downstream = manager
            .store()
            .unwrap()
            .list_direct_downstream(&self_neuron.id, 20, &HashSet::new())
            .unwrap();
        assert_eq!(downstream.len(), 6);
        assert_eq!(candidates.len(), 7);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_global_default_uses_existing_seven() {
        let (manager, root) = test_manager();
        for index in 0..7 {
            insert_plain(&manager, &format!("global-{index}"), "global");
        }
        let count_before = manager.list().unwrap().len();

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::global_default())
            .await
            .unwrap();

        assert_eq!(candidates.len(), DEFAULT_SELECT_N);
        assert_eq!(manager.list().unwrap().len(), count_before);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_honors_custom_neighborhood_policy() {
        let (manager, root) = test_manager();
        let self_neuron = insert_plain(&manager, "self", "self");
        let existing_child = insert_downstream(&manager, &self_neuron.id, "existing-child");
        let policy = NeighborhoodPoolPolicy {
            existing_downstream: 2,
            new_downstream: 1,
            fill_downstream_shortage: false,
            siblings: 0,
            upstream_depth: 0,
            global_top_weight: 0,
        };

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::Neighborhood {
                self_id: self_neuron.id.clone(),
                policy,
            })
            .await
            .unwrap();
        let candidate_ids: HashSet<&str> = candidates.iter().map(|n| n.id.as_str()).collect();
        let downstream = manager
            .store()
            .unwrap()
            .list_direct_downstream(&self_neuron.id, 20, &HashSet::new())
            .unwrap();

        assert_eq!(downstream.len(), 2);
        assert_eq!(candidates.len(), 3);
        assert!(candidate_ids.contains(self_neuron.id.as_str()));
        assert!(candidate_ids.contains(existing_child.id.as_str()));
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_appends_global_top_weight_neurons() {
        let (manager, root) = test_manager();
        let self_neuron = insert_plain(&manager, "self", "self");
        insert_downstream(&manager, &self_neuron.id, "child");
        // 高权重孤立节点：不在 self 邻域内，weight 远超其余节点，应被 top5 补充进池。
        let top = insert_plain(&manager, "top-weighted", "top");
        manager.adjust_weight(&top.id, 50.0).unwrap();

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::neighborhood_default(
                self_neuron.id.clone(),
            ))
            .await
            .unwrap();
        let ids: HashSet<&str> = candidates.iter().map(|n| n.id.as_str()).collect();
        assert!(
            ids.contains(top.id.as_str()),
            "global top weighted node must be appended to neighborhood pool"
        );
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_zero_top_weight_skips_global_append() {
        let (manager, root) = test_manager();
        let self_neuron = insert_plain(&manager, "self", "self");
        let top = insert_plain(&manager, "top-weighted", "top");
        manager.adjust_weight(&top.id, 50.0).unwrap();

        let candidates = manager
            .select_assistant_candidates(AssistantCandidateScope::Neighborhood {
                self_id: self_neuron.id.clone(),
                policy: NeighborhoodPoolPolicy {
                    existing_downstream: 0,
                    new_downstream: 1,
                    fill_downstream_shortage: false,
                    siblings: 0,
                    upstream_depth: 0,
                    global_top_weight: 0,
                },
            })
            .await
            .unwrap();
        let ids: HashSet<&str> = candidates.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(self_neuron.id.as_str()));
        assert!(
            !ids.contains(top.id.as_str()),
            "zero global_top_weight must not append top weighted node"
        );
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_rejects_zero_global_limit() {
        let (manager, root) = test_manager();
        let result = manager
            .select_assistant_candidates(AssistantCandidateScope::Global { limit: 0 })
            .await;
        assert!(result.is_err());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn assistant_candidates_rejects_new_downstream_above_batch_limit() {
        let (manager, root) = test_manager();
        let self_neuron = insert_plain(&manager, "self", "self");
        let result = manager
            .select_assistant_candidates(AssistantCandidateScope::Neighborhood {
                self_id: self_neuron.id,
                policy: NeighborhoodPoolPolicy {
                    existing_downstream: 0,
                    new_downstream: MAX_CREATE_NEURON_COUNT + 1,
                    fill_downstream_shortage: false,
                    siblings: 0,
                    upstream_depth: 0,
                    global_top_weight: 0,
                },
            })
            .await;
        assert!(result.is_err());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn select_candidates_under_creator_returns_seven() {
        let (manager, root) = test_manager();
        let creator = manager.ensure_creator().unwrap();
        let candidates = manager
            .select_candidates(CandidateQuery {
                n: DEFAULT_SELECT_N,
                source_id: Some(creator.id.clone()),
                min_new: 0,
            })
            .await
            .unwrap();
        assert_eq!(candidates.len(), 7);
        let downstream = manager
            .store()
            .unwrap()
            .list_direct_downstream(&creator.id, 10, &HashSet::new())
            .unwrap();
        assert_eq!(downstream.len(), 7);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ai_update_rejects_system_neuron() {
        let (manager, root) = test_manager();
        let creator = manager.ensure_creator().unwrap();
        let result = manager.update_content_for_ai(
            &creator.id,
            NeuronUpdate {
                desc: Some("changed".into()),
                content: None,
                ..Default::default()
            },
        );
        assert!(result.is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn select_one_from_falls_back_to_weight_without_selector() {
        let (manager, root) = test_manager();
        let low = insert_plain(&manager, "low", "low");
        let high = insert_plain(&manager, "high", "high");
        let low = manager.adjust_weight(&low.id, 1.0).unwrap();
        let high = manager.adjust_weight(&high.id, 9.0).unwrap();
        let selected = manager.select_one_from(&[low, high.clone()]).await.unwrap();
        assert_eq!(selected.id, high.id);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn create_neuron_ignores_model_weight_and_uses_zero() {
        let (manager, root) = test_manager();
        let source = insert_plain(&manager, "source", "source");
        let children = manager
            .create_neuron(
                CreateNeuronInput::Purpose("test purpose".into()),
                Some(&source.id),
                1,
            )
            .await
            .unwrap();
        assert_eq!(children.len(), 1);
        let child = &children[0];
        assert!((child.weight - 0.0).abs() < f64::EPSILON);
        let edge = manager
            .connections(&source.id)
            .unwrap()
            .into_iter()
            .find(|c| c.target == child.id)
            .unwrap();
        assert!((edge.weight - 0.0).abs() < f64::EPSILON);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn create_neuron_batch_returns_requested_count() {
        let (manager, root) = test_manager();
        let neurons = manager
            .create_neuron(CreateNeuronInput::Purpose("batch purpose".into()), None, 3)
            .await
            .unwrap();
        assert_eq!(neurons.len(), 3);
        assert!(neurons.iter().all(|n| n.system_type.is_none()));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn ensure_system_neuron_fills_own_downstream_not_creator() {
        let (manager, root) = test_manager();
        let creator = manager.ensure_creator().unwrap();
        let selector = manager
            .ensure_system_neuron(ASSISTANT_SELECT_NEURON, EnsureSystemOpts { reset: false })
            .await
            .unwrap();
        let selector_kids = manager
            .store()
            .unwrap()
            .list_direct_downstream(&selector.id, 20, &HashSet::new())
            .unwrap();
        assert_eq!(selector_kids.len(), DEFAULT_SELECT_N);
        let creator_kids = manager
            .store()
            .unwrap()
            .list_direct_downstream(&creator.id, 20, &HashSet::new())
            .unwrap();
        // Creator may gain kids when create_neuron picks a creation prompt; selector pool must be under selector.
        assert!(selector_kids.iter().all(|n| n.system_type.is_none()));
        assert!(!selector_kids
            .iter()
            .any(|n| creator_kids.iter().any(|c| c.id == n.id)));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn ensure_system_neuron_backfills_default_behavior_for_legacy_system_neuron() {
        let (manager, root) = test_manager();
        // 模拟老数据：裁决类系统神经元存在但 behavior 为空（早期版本创建时未写）。
        let legacy = manager
            .store()
            .unwrap()
            .create_neuron(NeuronCreate {
                desc: "主题匹配".into(),
                content: "match topic content".into(),
                weight: 0.0,
                system_type: Some("assistant_match_topic".into()),
                tool_ids: vec![],
                lineage_parent_id: None,
                variant_state: None,
            })
            .unwrap();
        assert!(legacy.behavior.is_none());

        // ensure 命中已有：自动补写默认 behavior（Fixed + 契约段）。
        let ensured = manager
            .ensure_system_neuron("assistant_match_topic", EnsureSystemOpts { reset: false })
            .await
            .unwrap();
        let behavior = ensured.behavior.expect("behavior should be backfilled");
        assert_eq!(behavior.selection, SelectionPolicy::Fixed);
        assert_eq!(behavior.tools, ToolPolicy::None);
        assert_eq!(behavior.insert_id.as_deref(), Some("assistant.match_topic"));

        // 幂等：已有 behavior 不被覆盖。
        let again = manager
            .ensure_system_neuron("assistant_match_topic", EnsureSystemOpts { reset: false })
            .await
            .unwrap();
        assert_eq!(again.behavior, Some(behavior));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ensure_creator_uses_default_prompt_without_config() {
        let (manager, root) = test_manager();
        let creator = manager.ensure_creator().unwrap();
        assert_eq!(creator.system_type.as_deref(), Some(CREATOR_SYSTEM_TYPE));
        assert!(!creator.content.trim().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    fn seed_for(system_type: &str) -> String {
        SYSTEM_PROMPT_SEEDS
            .iter()
            .find(|(ty, _)| *ty == system_type)
            .map(|(_, content)| content.to_string())
            .unwrap_or_else(|| panic!("missing built-in seed for {system_type}"))
    }

    #[tokio::test]
    async fn ensure_system_neuron_uses_builtin_seed_without_model_call() {
        let (manager, root) = test_manager();
        let selector = manager
            .ensure_system_neuron(ASSISTANT_SELECT_NEURON, EnsureSystemOpts { reset: false })
            .await
            .unwrap();
        // content 必须是内置种子（LLM 分支产物为 "content-N" 格式，可用于区分）。
        assert_eq!(selector.content, seed_for(ASSISTANT_SELECT_NEURON));
        assert_eq!(selector.desc, ASSISTANT_SELECT_NEURON);
        assert!(selector.tool_ids.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn ensure_system_neuron_builtin_seed_preserves_default_behavior() {
        let (manager, root) = test_manager();
        let ensured = manager
            .ensure_system_neuron(
                "assistant_complete_scope",
                EnsureSystemOpts { reset: false },
            )
            .await
            .unwrap();
        assert_eq!(ensured.content, seed_for("assistant_complete_scope"));
        // 创建分支返回对象不带 behavior（behavior 是落库补写）；从 store 重取断言。
        let stored = manager
            .get(&ensured.id)
            .unwrap()
            .expect("system neuron should exist");
        let behavior = stored
            .behavior
            .expect("裁决类种子创建后应补写默认 behavior");
        assert_eq!(behavior.selection, SelectionPolicy::Fixed);
        assert_eq!(behavior.tools, ToolPolicy::None);
        assert_eq!(
            behavior.insert_id.as_deref(),
            Some("assistant.complete_scope")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn ensure_system_neuron_respects_config_system_prompt_override() {
        let (manager, root) = test_manager();
        fs::write(
            root.join("config.json"),
            r#"{"neurons":{"bootstrap":{"create_neuron_prompt":"create a neuron","system_prompts":{"assistant_select_neuron":"override prompt"}}}}"#,
        )
        .unwrap();
        let selector = manager
            .ensure_system_neuron(ASSISTANT_SELECT_NEURON, EnsureSystemOpts { reset: false })
            .await
            .unwrap();
        assert_eq!(selector.content, "override prompt");
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn ensure_system_neuron_falls_back_to_llm_for_custom_type() {
        let (manager, root) = test_manager();
        let custom = manager
            .ensure_system_neuron("custom_type", EnsureSystemOpts { reset: false })
            .await
            .unwrap();
        assert_eq!(custom.system_type.as_deref(), Some("custom_type"));
        assert!(
            custom.content.starts_with("content-"),
            "自定义 type 无内置种子应走 LLM 生成，got: {}",
            custom.content
        );
        fs::remove_dir_all(root).unwrap();
    }

    // ── Creator self-iteration pool ─────────────────────────

    /// Build the creator plus its 7-variant pool.
    async fn seed_creator_pool(manager: &NeuronManager) -> String {
        let creator = manager.ensure_creator().unwrap();
        manager
            .select_candidates(CandidateQuery {
                n: DEFAULT_SELECT_N,
                source_id: Some(creator.id.clone()),
                min_new: 0,
            })
            .await
            .unwrap();
        creator.id
    }

    #[tokio::test]
    async fn create_neuron_attributes_lineage_and_usage() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let before_len = {
            let store = manager.store().unwrap();
            store.get_variants(&creator_id, false).unwrap().len()
        };
        assert_eq!(before_len, DEFAULT_SELECT_N);

        let children = manager
            .create_neuron(CreateNeuronInput::Purpose("lineage test".into()), None, 1)
            .await
            .unwrap();
        assert_eq!(children.len(), 1);
        let parent_id = manager
            .store()
            .unwrap()
            .lineage_parent_id_of(&children[0].id)
            .unwrap()
            .expect("child must carry lineage");
        assert_ne!(parent_id, creator_id);

        let used = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == parent_id)
                .expect("parent variant must exist")
        };
        assert_eq!(used.use_count, 1);
        assert!(used.last_used_at.is_some());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn create_neuron_filling_creator_links_lineage_to_creator() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let children = manager
            .create_neuron(
                CreateNeuronInput::Purpose("fill test".into()),
                Some(&creator_id),
                1,
            )
            .await
            .unwrap();
        let parent_id = manager
            .store()
            .unwrap()
            .lineage_parent_id_of(&children[0].id)
            .unwrap()
            .expect("seed-born child must carry lineage");
        assert_eq!(parent_id, creator_id);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn observing_variant_promotes_after_use_and_rolls_back_on_regression() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let (a, b) = {
            let store = manager.store().unwrap();
            let variants = store.get_variants(&creator_id, false).unwrap();
            let a = variants[0].neuron.id.clone();
            let b = variants[1].neuron.id.clone();
            store.set_variant_state(&a, Some("observing")).unwrap();
            store.set_variant_state(&b, Some("observing")).unwrap();
            (a, b)
        };

        // Idle observing slot stays put.
        manager.maybe_evolve_creator_variants().await.unwrap();
        let idle_state = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == a)
                .unwrap()
                .variant_state
        };
        assert_eq!(idle_state.as_deref(), Some("observing"));

        // Regression (negative delta) without history: stays observing.
        manager
            .store()
            .unwrap()
            .accumulate_variant_delta(&a, -1.0)
            .unwrap();
        manager.maybe_evolve_creator_variants().await.unwrap();
        let a_state = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == a)
                .unwrap()
                .variant_state
        };
        assert_eq!(a_state.as_deref(), Some("observing"));

        // Clear the regression, then a used observing variant gets promoted.
        manager
            .store()
            .unwrap()
            .accumulate_variant_delta(&a, 1.0)
            .unwrap();
        manager
            .store()
            .unwrap()
            .increment_variant_usage(&b)
            .unwrap();
        manager.maybe_evolve_creator_variants().await.unwrap();
        let b_state = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == b)
                .unwrap()
                .variant_state
        };
        assert_eq!(b_state.as_deref(), Some("active"));
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn active_variant_rewrites_at_threshold_and_moves_to_observing() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let (target, original_content) = {
            let store = manager.store().unwrap();
            let variants = store.get_variants(&creator_id, false).unwrap();
            let target = variants[0].neuron.id.clone();
            let original_content = variants[0].neuron.content.clone();
            for _ in 0..3 {
                store.increment_variant_usage(&target).unwrap();
            }
            store.accumulate_variant_delta(&target, 2.0).unwrap();
            (target, original_content)
        };

        manager.maybe_evolve_creator_variants().await.unwrap();

        let (state, content) = {
            let store = manager.store().unwrap();
            let rewritten = store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == target)
                .unwrap();
            (rewritten.variant_state, rewritten.neuron.content)
        };
        assert_eq!(state.as_deref(), Some("observing"));
        assert_ne!(content, original_content);
        let version = manager
            .store()
            .unwrap()
            .latest_version_of(&target)
            .unwrap()
            .expect("evolve must archive a version");
        assert_eq!(version.source, "evolve");
        assert_eq!(version.content, original_content);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn manual_edited_variant_is_locked_from_rewrite_and_elimination() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let (target, original_content) = {
            let store = manager.store().unwrap();
            let variants = store.get_variants(&creator_id, false).unwrap();
            let target = variants[0].neuron.id.clone();
            let original_content = variants[0].neuron.content.clone();
            for _ in 0..3 {
                store.increment_variant_usage(&target).unwrap();
            }
            store.accumulate_variant_delta(&target, 2.0).unwrap();
            store.set_manual_edited(&target, true).unwrap();
            (target, original_content)
        };

        manager.maybe_evolve_creator_variants().await.unwrap();

        let (state, content, has_version) = {
            let store = manager.store().unwrap();
            let locked = store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == target)
                .unwrap();
            (
                locked.variant_state,
                locked.neuron.content,
                store.latest_version_of(&target).unwrap().is_some(),
            )
        };
        // Seed variants carry NULL state, which means active (not observing).
        assert_ne!(state.as_deref(), Some("observing"));
        assert_eq!(content, original_content);
        assert!(!has_version);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn eliminated_variant_rolls_back_to_archived_version() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        let target = {
            let store = manager.store().unwrap();
            let variants = store.get_variants(&creator_id, false).unwrap();
            let target = variants[0].neuron.id.clone();
            store
                .insert_neuron_version(&target, "archived-v1", "seed", None)
                .unwrap();
            store
                .update_neuron(
                    &target,
                    NeuronUpdate {
                        desc: None,
                        content: Some("current-v2".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            for _ in 0..3 {
                store.accumulate_variant_delta(&target, -1.0).unwrap();
            }
            target
        };

        manager.maybe_evolve_creator_variants().await.unwrap();

        let rolled_content = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator_id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == target)
                .unwrap()
                .neuron
                .content
        };
        assert_eq!(rolled_content, "archived-v1");
        let version = manager
            .store()
            .unwrap()
            .latest_version_of(&target)
            .unwrap()
            .expect("rollback must be recorded");
        assert_eq!(version.source, "rollback");
        assert_eq!(version.content, "archived-v1");
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn legacy_null_variant_state_is_treated_as_active() {
        let (manager, root) = test_manager();
        let creator_id = seed_creator_pool(&manager).await;
        {
            let store = manager.store().unwrap();
            assert_eq!(
                store.get_variants(&creator_id, true).unwrap().len(),
                DEFAULT_SELECT_N
            );
        }

        manager.maybe_evolve_creator_variants().await.unwrap();

        let after = {
            let store = manager.store().unwrap();
            store.get_variants(&creator_id, false).unwrap()
        };
        assert_eq!(after.len(), DEFAULT_SELECT_N);
        assert!(after.iter().all(|v| v.variant_state.is_none()));
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn admin_update_marks_variant_manual_edited() {
        let (manager, root) = test_manager();
        let creator = manager.ensure_creator().unwrap();
        let variant = manager
            .create_plain(
                NeuronCreate {
                    desc: "v".into(),
                    content: "original".into(),
                    ..Default::default()
                },
                Some(&creator.id),
            )
            .unwrap();
        manager
            .update_content_for_admin(
                &variant.id,
                NeuronUpdate {
                    desc: None,
                    content: Some("edited".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let edited = {
            let store = manager.store().unwrap();
            store
                .get_variants(&creator.id, false)
                .unwrap()
                .into_iter()
                .find(|v| v.neuron.id == variant.id)
                .unwrap()
        };
        assert!(edited.manual_edited);
        assert_eq!(edited.neuron.content, "edited");
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    // ── Capacity & low-value recycling ─────────────────────────

    fn write_config_with_capacity(root: &std::path::Path, capacity: usize) {
        fs::write(
            root.join("config.json"),
            format!(
                r#"{{"neurons":{{"bootstrap":{{"create_neuron_prompt":"create a neuron"}}}},"neuron":{{"capacity":{capacity}}}}}"#
            ),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn recycle_if_over_capacity_recycles_lowest_value_and_exempts_system() {
        let (manager, root) = test_manager();
        write_config_with_capacity(&root, 3);

        for index in 0..5 {
            insert_plain(&manager, &format!("n{index}"), "plain");
        }
        manager
            .store()
            .unwrap()
            .create_neuron(NeuronCreate {
                desc: "sys".into(),
                content: "prompt".into(),
                system_type: Some("sys_type".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(manager.list().unwrap().len(), 6);

        let recycled = manager.recycle_if_over_capacity().unwrap();
        assert_eq!(recycled, 3);
        // 2 个普通 + 1 个系统保留，恰好等于容量 3。
        assert_eq!(manager.list().unwrap().len(), 3);
        assert!(manager
            .get_neuron_by_system_type("sys_type")
            .unwrap()
            .is_some());

        // 幂等：未超容量返回 0。
        assert_eq!(manager.recycle_if_over_capacity().unwrap(), 0);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn recycle_spares_used_neurons() {
        let (manager, root) = test_manager();
        write_config_with_capacity(&root, 2);

        let used = insert_plain(&manager, "used", "used");
        insert_plain(&manager, "low-a", "a");
        insert_plain(&manager, "low-b", "b");
        manager.store().unwrap().mark_used(&used.id).unwrap();

        let recycled = manager.recycle_if_over_capacity().unwrap();
        assert_eq!(recycled, 1);
        let remaining: Vec<String> = manager.list().unwrap().into_iter().map(|n| n.id).collect();
        assert!(remaining.contains(&used.id), "used neuron must survive");
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn select_one_marks_usage_as_active_signal() {
        let (manager, root) = test_manager();
        let n = insert_plain(&manager, "target", "target");
        let selected = manager.select_one_from(&[n.clone()]).await.unwrap();
        assert_eq!(selected.id, n.id);
        let stored = manager.get(&n.id).unwrap().unwrap();
        assert_eq!(stored.use_count, 1);
        assert!(stored.last_used_at.is_some());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    // ── 模选回挂边（link back source → target）─────────────────

    #[tokio::test]
    async fn link_back_creates_edge_when_target_not_direct_downstream() {
        let caller = Arc::new(PickIdModelCaller {
            target_id: Mutex::new(String::new()),
        });
        let (manager, root) = test_manager_with(caller.clone() as Arc<dyn NeuronModelCaller>);
        insert_selector(&manager);
        let anchor = insert_plain(&manager, "anchor", "anchor content");
        let target = insert_plain(&manager, "target", "target content");
        *caller.target_id.lock().unwrap() = target.id.clone();

        let selected = manager
            .select_one_from_with_history(&[target.clone()], &[], Some(anchor.id.as_str()))
            .await
            .unwrap();
        assert_eq!(selected.id, target.id);
        assert!(
            manager
                .store()
                .unwrap()
                .connection_exists(&anchor.id, &target.id)
                .unwrap(),
            "模选命中非直接下游应新建 anchor -> target 边"
        );
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn link_back_skips_existing_direct_edge() {
        let caller = Arc::new(PickIdModelCaller {
            target_id: Mutex::new(String::new()),
        });
        let (manager, root) = test_manager_with(caller.clone() as Arc<dyn NeuronModelCaller>);
        insert_selector(&manager);
        let anchor = insert_plain(&manager, "anchor", "anchor content");
        let target = insert_downstream(&manager, &anchor.id, "child");
        // 给既有边加权重，验证回挂不会重写为 0。
        manager
            .store()
            .unwrap()
            .adjust_connection_weight(&anchor.id, &target.id, 5.0)
            .unwrap();
        *caller.target_id.lock().unwrap() = target.id.clone();

        let selected = manager
            .select_one_from_with_history(&[target.clone()], &[], Some(anchor.id.as_str()))
            .await
            .unwrap();
        assert_eq!(selected.id, target.id);
        let edges = manager.store().unwrap().get_connections(&anchor.id).unwrap();
        let edge = edges
            .iter()
            .find(|e| e.source == anchor.id && e.target == target.id)
            .expect("anchor -> target 边应存在");
        assert_eq!(edge.weight, 5.0, "已存在直接边不应被回挂重写权重");
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn link_back_skips_self_loop() {
        let caller = Arc::new(PickIdModelCaller {
            target_id: Mutex::new(String::new()),
        });
        let (manager, root) = test_manager_with(caller.clone() as Arc<dyn NeuronModelCaller>);
        insert_selector(&manager);
        let anchor = insert_plain(&manager, "anchor", "anchor content");
        *caller.target_id.lock().unwrap() = anchor.id.clone();

        let selected = manager
            .select_one_from_with_history(&[anchor.clone()], &[], Some(anchor.id.as_str()))
            .await
            .unwrap();
        assert_eq!(selected.id, anchor.id);
        assert!(
            !manager
                .store()
                .unwrap()
                .connection_exists(&anchor.id, &anchor.id)
                .unwrap(),
            "target == source 时不应自环"
        );
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn link_back_skipped_without_anchor() {
        let caller = Arc::new(PickIdModelCaller {
            target_id: Mutex::new(String::new()),
        });
        let (manager, root) = test_manager_with(caller.clone() as Arc<dyn NeuronModelCaller>);
        insert_selector(&manager);
        let anchor = insert_plain(&manager, "anchor", "anchor content");
        let target = insert_plain(&manager, "target", "target content");
        *caller.target_id.lock().unwrap() = target.id.clone();

        let selected = manager
            .select_one_from_with_history(&[target.clone()], &[], None)
            .await
            .unwrap();
        assert_eq!(selected.id, target.id);
        assert!(
            !manager
                .store()
                .unwrap()
                .connection_exists(&anchor.id, &target.id)
                .unwrap(),
            "无锚点（select_one_from / Global 首轮）不应建边"
        );
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn link_back_skipped_on_global_first_round() {
        let caller = Arc::new(PickIdModelCaller {
            target_id: Mutex::new(String::new()),
        });
        let (manager, root) = test_manager_with(caller.clone() as Arc<dyn NeuronModelCaller>);
        insert_selector(&manager);
        // 预置恰好 3 条普通节点（Global 候选池只含 system_type IS NULL），避免触发补池模型调用。
        let anchor = insert_plain(&manager, "anchor", "anchor content");
        let target = insert_plain(&manager, "target", "target content");
        insert_plain(&manager, "extra", "extra content");
        *caller.target_id.lock().unwrap() = target.id.clone();

        let selected = manager
            .select_role(&[], AssistantCandidateScope::Global { limit: 3 })
            .await
            .unwrap();
        assert_eq!(selected.id, target.id);
        assert!(
            !manager
                .store()
                .unwrap()
                .connection_exists(&anchor.id, &target.id)
                .unwrap(),
            "Global 首轮无锚点，不应建边"
        );
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    // ── 统一管理：分页 / 搜索 / 类型筛选 ─────────────────────────

    #[tokio::test]
    async fn list_neurons_page_paging_search_kind() {
        let (manager, root) = test_manager();
        // 3 条普通 + 2 条系统。
        for i in 0..3 {
            insert_plain(&manager, &format!("plain-{i}"), "c");
        }
        for (i, st) in ["session.a", "session.b"].iter().enumerate() {
            manager
                .store()
                .unwrap()
                .create_neuron(NeuronCreate {
                    desc: format!("sys-{i}"),
                    content: "c".into(),
                    system_type: Some((*st).into()),
                    ..Default::default()
                })
                .unwrap();
        }

        // 分页：page0 size2 → 2 条、has_more、total=5；page1 不越界。
        let page0 = manager
            .list_neurons_page(0, 2, None, NeuronKindFilter::All)
            .unwrap();
        assert_eq!(page0.items.len(), 2);
        assert!(page0.has_more);
        assert_eq!(page0.total, 5);
        let page2 = manager
            .list_neurons_page(2, 2, None, NeuronKindFilter::All)
            .unwrap();
        assert_eq!(page2.items.len(), 1);
        assert!(!page2.has_more);

        // 搜索：desc 模糊命中；不存在的词命中 0。
        let hit = manager
            .list_neurons_page(0, 10, Some("plain-1"), NeuronKindFilter::All)
            .unwrap();
        assert_eq!(hit.items.len(), 1);
        assert_eq!(hit.items[0].desc, "plain-1");
        let miss = manager
            .list_neurons_page(0, 10, Some("不存在"), NeuronKindFilter::All)
            .unwrap();
        assert!(miss.items.is_empty());

        // 类型筛选：系统 2 条、普通 3 条。
        let sys = manager
            .list_neurons_page(0, 10, None, NeuronKindFilter::System)
            .unwrap();
        assert_eq!(sys.items.len(), 2);
        assert!(sys.items.iter().all(|n| n.system_type.is_some()));
        let normal = manager
            .list_neurons_page(0, 10, None, NeuronKindFilter::Normal)
            .unwrap();
        assert_eq!(normal.items.len(), 3);
        assert!(normal.items.iter().all(|n| n.system_type.is_none()));

        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn set_system_type_for_admin_bind_switch_unbind() {
        let (manager, root) = test_manager();
        let n1 = insert_plain(&manager, "one", "c");
        let n2 = insert_plain(&manager, "two", "c");

        // 绑定。
        let bound = manager
            .set_system_type_for_admin(&n1.id, Some("session.demo"))
            .unwrap();
        assert_eq!(bound.system_type.as_deref(), Some("session.demo"));

        // 唯一冲突：同一 system_type 不能绑到另一条。
        let err = manager
            .set_system_type_for_admin(&n2.id, Some("session.demo"))
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));

        // 换绑：n2 换新类型成功，n1 保持原类型。
        let switched = manager
            .set_system_type_for_admin(&n2.id, Some("session.other"))
            .unwrap();
        assert_eq!(switched.system_type.as_deref(), Some("session.other"));
        assert_eq!(
            manager
                .get_neuron_by_system_type("session.demo")
                .unwrap()
                .unwrap()
                .id,
            n1.id
        );

        // 取消绑定（空白视为 None）。
        let unbound = manager
            .set_system_type_for_admin(&n1.id, Some("  "))
            .unwrap();
        assert!(unbound.system_type.is_none());

        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn update_behavior_for_admin_requires_system_type() {
        let (manager, root) = test_manager();
        let behavior = SessionBehavior {
            selection: SelectionPolicy::Fixed,
            tools: ToolPolicy::None,
            insert_id: None,
        };

        // 普通神经元无 system_type：拒绝。
        let plain = insert_plain(&manager, "plain", "c");
        let err = manager
            .update_behavior_for_admin(&plain.id, behavior.clone())
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));

        // 绑定系统类型后可写并落库。
        let sys = manager
            .set_system_type_for_admin(&plain.id, Some("session.demo"))
            .unwrap();
        let updated = manager
            .update_behavior_for_admin(&sys.id, behavior.clone())
            .unwrap();
        assert_eq!(updated.behavior, Some(behavior));

        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }
