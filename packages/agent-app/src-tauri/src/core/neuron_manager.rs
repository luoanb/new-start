use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use serde_json::Value;

use super::{
    error::{AppError, AppResult},
    models::{
        CandidateQuery, Connection, GeneratedNeuronDraft, Neuron, NeuronCreate, NeuronUpdate,
    },
    neuron_config::NeuronConfigReader,
    neuron_model::NeuronModelCaller,
    neuron_store::NeuronStore,
    tool_registry::{Tool, ToolRegistry},
};

const CREATOR_SYSTEM_TYPE: &str = "create_neuron";

pub struct NeuronManager {
    store: Arc<Mutex<NeuronStore>>,
    model_caller: Arc<dyn NeuronModelCaller>,
    config: NeuronConfigReader,
    creator_id: Mutex<Option<String>>,
}

impl std::fmt::Debug for NeuronManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeuronManager").finish_non_exhaustive()
    }
}

impl NeuronManager {
    pub fn new(
        store: Arc<Mutex<NeuronStore>>,
        model_caller: Arc<dyn NeuronModelCaller>,
        config: NeuronConfigReader,
    ) -> Self {
        Self {
            store,
            model_caller,
            config,
            creator_id: Mutex::new(None),
        }
    }

    pub fn register_ai_tools(self: &Arc<Self>, registry: &mut ToolRegistry) {
        registry.register(GetNeuronTool::new(Arc::clone(self)));
        registry.register(ListNeuronsTool::new(Arc::clone(self)));
        registry.register(UpdateNeuronTool::new(Arc::clone(self)));
        registry.register(GetNetworkTool::new(Arc::clone(self)));
        registry.register(CreateDownstreamNeuronTool::new(Arc::clone(self)));
        registry.register(SelectNeuronCandidatesTool::new(Arc::clone(self)));
    }

    pub fn get_neuron(&self, id: &str) -> AppResult<Option<Neuron>> {
        self.store()?.get_neuron(id)
    }

    pub fn list_neurons(&self) -> AppResult<Vec<Neuron>> {
        self.store()?.list_neurons()
    }

    pub fn get_connections(&self, id: &str) -> AppResult<Vec<Connection>> {
        self.store()?.get_connections(id)
    }

    pub fn get_network(&self, id: &str, max_depth: usize) -> AppResult<Vec<Neuron>> {
        self.store()?.get_network(id, max_depth)
    }

    pub fn create_for_admin(&self, create: NeuronCreate) -> AppResult<Neuron> {
        self.store()?.create_neuron(create)
    }

    pub fn update_for_ai(&self, id: &str, update: NeuronUpdate) -> AppResult<Neuron> {
        let store = self.store()?;
        let neuron = store
            .get_neuron(id)?
            .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))?;
        if neuron.system_type.is_some() {
            return Err(AppError::InvalidInput(
                "AI tools cannot update system neurons".into(),
            ));
        }
        store.update_neuron(id, update)
    }

    pub fn update_for_admin(&self, id: &str, update: NeuronUpdate) -> AppResult<Neuron> {
        self.store()?.update_neuron(id, update)
    }

    pub fn adjust_weight(&self, id: &str, delta: f64) -> AppResult<Neuron> {
        self.store()?.adjust_weight(id, delta)
    }

    pub fn delete_for_admin(&self, id: &str) -> AppResult<bool> {
        let deleted = self.store()?.delete_neuron(id)?;
        if deleted {
            let mut creator_id = self.creator_id.lock().map_err(lock_error)?;
            if creator_id.as_deref() == Some(id) {
                *creator_id = None;
            }
        }
        Ok(deleted)
    }

    pub fn link_for_admin(&self, source: &str, target: &str, weight: f64) -> AppResult<Connection> {
        self.store()?.link(source, target, weight)
    }

    pub fn unlink_for_admin(&self, source: &str, target: &str) -> AppResult<bool> {
        self.store()?.unlink(source, target)
    }

    pub fn set_system_type_for_admin(
        &self,
        id: &str,
        system_type: Option<&str>,
    ) -> AppResult<Neuron> {
        let neuron = self.store()?.set_system_type(id, system_type)?;
        let mut creator_id = self.creator_id.lock().map_err(lock_error)?;
        if system_type == Some(CREATOR_SYSTEM_TYPE) {
            *creator_id = Some(id.to_string());
        } else if creator_id.as_deref() == Some(id) {
            *creator_id = None;
        }
        Ok(neuron)
    }

    pub fn set_tool_ids_for_admin(&self, id: &str, tool_ids: Vec<String>) -> AppResult<Neuron> {
        self.store()?.set_tool_ids(id, tool_ids)
    }

    pub fn create_downstream(
        &self,
        source_id: &str,
        create: NeuronCreate,
        edge_weight: f64,
    ) -> AppResult<(Neuron, Connection)> {
        self.store()?
            .create_downstream_neuron(source_id, create, edge_weight)
    }

    pub async fn select_candidates(&self, query: CandidateQuery) -> AppResult<Vec<Neuron>> {
        if query.min_new > query.n {
            return Err(AppError::InvalidInput(
                "min_new must be less than or equal to n".into(),
            ));
        }
        if query.n == 0 {
            return Ok(Vec::new());
        }

        let source_id = self
            .resolve_source(query.source_id.as_deref(), query.system_type.as_deref())
            .await?;
        let mut selected = Vec::with_capacity(query.n);
        let mut selected_ids = HashSet::new();

        for _ in 0..query.min_new {
            let neuron = self.create_generated_neuron(source_id.as_deref()).await?;
            selected_ids.insert(neuron.id.clone());
            selected.push(neuron);
        }

        let remaining = query.n - selected.len();
        if remaining > 0 {
            let existing = {
                let store = self.store()?;
                match source_id.as_deref() {
                    Some(source_id) => {
                        store.list_direct_downstream(source_id, remaining, &selected_ids)?
                    }
                    None => store.list_global_candidates(remaining, &selected_ids)?,
                }
            };
            for neuron in existing {
                selected_ids.insert(neuron.id.clone());
                selected.push(neuron);
            }
        }

        while selected.len() < query.n {
            let neuron = self.create_generated_neuron(source_id.as_deref()).await?;
            if selected_ids.insert(neuron.id.clone()) {
                selected.push(neuron);
            }
        }

        Ok(selected)
    }

    pub async fn bootstrap_creator_candidates(&self) -> AppResult<Vec<Neuron>> {
        self.select_candidates(CandidateQuery {
            n: 7,
            source_id: None,
            system_type: Some(CREATOR_SYSTEM_TYPE.into()),
            min_new: 0,
        })
        .await
    }

    pub fn ensure_creator_for_admin(&self) -> AppResult<Neuron> {
        self.ensure_creator_neuron()
    }

    async fn resolve_source(
        &self,
        source_id: Option<&str>,
        system_type: Option<&str>,
    ) -> AppResult<Option<String>> {
        if let Some(source_id) = source_id {
            if self.get_neuron(source_id)?.is_none() {
                return Err(AppError::NeuronNotFound(source_id.to_string()));
            }
            return Ok(Some(source_id.to_string()));
        }
        let Some(system_type) = system_type else {
            return Ok(None);
        };
        if system_type == CREATOR_SYSTEM_TYPE {
            return Ok(Some(self.ensure_creator_neuron()?.id));
        }
        self.store()?
            .get_neuron_by_system_type(system_type)?
            .map(|neuron| Some(neuron.id))
            .ok_or_else(|| {
                AppError::InvalidInput(format!("System neuron type not found: {system_type}"))
            })
    }

    fn ensure_creator_neuron(&self) -> AppResult<Neuron> {
        let mut cached_id = self.creator_id.lock().map_err(lock_error)?;
        if let Some(id) = cached_id.clone() {
            if let Some(neuron) = self.get_neuron(&id)? {
                if neuron.system_type.as_deref() == Some(CREATOR_SYSTEM_TYPE) {
                    return Ok(neuron);
                }
            }
            *cached_id = None;
        }

        if let Some(neuron) = self
            .store()?
            .get_neuron_by_system_type(CREATOR_SYSTEM_TYPE)?
        {
            *cached_id = Some(neuron.id.clone());
            return Ok(neuron);
        }

        let prompt = self
            .config
            .create_neuron_prompt()
            .map_err(|error| AppError::NeuronBootstrapFailed(error.to_string()))?;
        let neuron = self.store()?.create_neuron(NeuronCreate {
            desc: "创建神经元".into(),
            content: prompt,
            weight: 0.0,
            system_type: Some(CREATOR_SYSTEM_TYPE.into()),
            tool_ids: Vec::new(),
        })?;
        *cached_id = Some(neuron.id.clone());
        Ok(neuron)
    }

    async fn create_generated_neuron(&self, source_id: Option<&str>) -> AppResult<Neuron> {
        let creator = self.ensure_creator_neuron()?;
        let user_prompt = match source_id {
            Some(source_id) => format!(
                "Create one downstream neuron for source_id {source_id}. Return only JSON with desc, content, weight, and tool_ids."
            ),
            None => {
                "Create one neuron. Return only JSON with desc, content, weight, and tool_ids."
                    .to_string()
            }
        };
        let output = self
            .model_caller
            .call_model(&creator.content, &user_prompt)
            .await?;
        let draft: GeneratedNeuronDraft = serde_json::from_str(output.trim()).map_err(|error| {
            AppError::NeuronBootstrapFailed(format!("Invalid generated neuron JSON: {error}"))
        })?;
        if draft.desc.trim().is_empty()
            || draft.content.trim().is_empty()
            || !draft.weight.is_finite()
        {
            return Err(AppError::NeuronBootstrapFailed(
                "Generated neuron must have non-empty desc/content and finite weight".into(),
            ));
        }
        let create = NeuronCreate {
            desc: draft.desc,
            content: draft.content,
            weight: draft.weight,
            system_type: None,
            tool_ids: draft.tool_ids,
        };
        match source_id {
            Some(source_id) => self
                .create_downstream(source_id, create, 1.0)
                .map(|(neuron, _)| neuron),
            None => self.create_for_admin(create),
        }
    }

    fn store(&self) -> AppResult<std::sync::MutexGuard<'_, NeuronStore>> {
        self.store.lock().map_err(lock_error)
    }
}

fn lock_error<T: std::fmt::Display>(error: T) -> AppError {
    AppError::StorageError(format!("Lock error: {error}"))
}

struct GetNeuronTool {
    manager: Arc<NeuronManager>,
}

impl GetNeuronTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for GetNeuronTool {
    fn name(&self) -> &str {
        "get_neuron"
    }

    fn description(&self) -> &str {
        "Get details of a neuron by ID, including its connections"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {"id": {"type": "string", "description": "Neuron ID"}},
            "required": ["id"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = required_str(&args, "id")?;
        let neuron = self
            .manager
            .get_neuron(id)?
            .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))?;
        let connections = self.manager.get_connections(id)?;
        Ok(format!(
            "{}\nConnections: {}",
            serde_json::to_string(&neuron).map_err(|e| AppError::StorageError(e.to_string()))?,
            serde_json::to_string(&connections)
                .map_err(|e| AppError::StorageError(e.to_string()))?
        ))
    }
}

struct ListNeuronsTool {
    manager: Arc<NeuronManager>,
}

impl ListNeuronsTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for ListNeuronsTool {
    fn name(&self) -> &str {
        "list_neurons"
    }

    fn description(&self) -> &str {
        "List all neurons"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn execute(&self, _args: Value) -> AppResult<String> {
        serde_json::to_string(&self.manager.list_neurons()?)
            .map_err(|e| AppError::StorageError(e.to_string()))
    }
}

struct UpdateNeuronTool {
    manager: Arc<NeuronManager>,
}

impl UpdateNeuronTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for UpdateNeuronTool {
    fn name(&self) -> &str {
        "update_neuron"
    }

    fn description(&self) -> &str {
        "Update a regular neuron's description or content"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "desc": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = required_str(&args, "id")?;
        let update = NeuronUpdate {
            desc: args.get("desc").and_then(Value::as_str).map(str::to_string),
            content: args
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        let neuron = self.manager.update_for_ai(id, update)?;
        serde_json::to_string(&neuron).map_err(|e| AppError::StorageError(e.to_string()))
    }
}

struct GetNetworkTool {
    manager: Arc<NeuronManager>,
}

impl GetNetworkTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for GetNetworkTool {
    fn name(&self) -> &str {
        "get_network"
    }

    fn description(&self) -> &str {
        "BFS traverse the neuron network from a seed up to max_depth"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "max_depth": {"type": "integer", "minimum": 0, "default": 3}
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let id = required_str(&args, "id")?;
        let max_depth = args.get("max_depth").and_then(Value::as_u64).unwrap_or(3) as usize;
        serde_json::to_string(&self.manager.get_network(id, max_depth)?)
            .map_err(|e| AppError::StorageError(e.to_string()))
    }
}

struct CreateDownstreamNeuronTool {
    manager: Arc<NeuronManager>,
}

impl CreateDownstreamNeuronTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for CreateDownstreamNeuronTool {
    fn name(&self) -> &str {
        "create_downstream_neuron"
    }

    fn description(&self) -> &str {
        "Create a regular neuron and connect it as a direct downstream neuron"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "source_id": {"type": "string"},
                "desc": {"type": "string"},
                "content": {"type": "string"},
                "weight": {"type": "number", "default": 0},
                "edge_weight": {"type": "number", "default": 1}
            },
            "required": ["source_id", "desc"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let source_id = required_str(&args, "source_id")?;
        let desc = required_str(&args, "desc")?;
        let create = NeuronCreate {
            desc: desc.to_string(),
            content: args
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            weight: args.get("weight").and_then(Value::as_f64).unwrap_or(0.0),
            system_type: None,
            tool_ids: Vec::new(),
        };
        let edge_weight = args
            .get("edge_weight")
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        let (neuron, connection) =
            self.manager
                .create_downstream(source_id, create, edge_weight)?;
        serde_json::to_string(&(neuron, connection))
            .map_err(|e| AppError::StorageError(e.to_string()))
    }
}

struct SelectNeuronCandidatesTool {
    manager: Arc<NeuronManager>,
}

impl SelectNeuronCandidatesTool {
    fn new(manager: Arc<NeuronManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for SelectNeuronCandidatesTool {
    fn name(&self) -> &str {
        "select_neuron_candidates"
    }

    fn description(&self) -> &str {
        "Select high-weight neuron candidates and create missing candidates"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "n": {"type": "integer", "minimum": 0},
                "source_id": {"type": "string"},
                "system_type": {"type": "string"},
                "min_new": {"type": "integer", "minimum": 0, "default": 0}
            },
            "required": ["n"]
        })
    }

    async fn execute(&self, args: Value) -> AppResult<String> {
        let n = args
            .get("n")
            .and_then(Value::as_u64)
            .ok_or_else(|| AppError::InvalidInput("Missing or invalid: n".into()))?
            as usize;
        let query = CandidateQuery {
            n,
            source_id: args
                .get("source_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            system_type: args
                .get("system_type")
                .and_then(Value::as_str)
                .map(str::to_string),
            min_new: args.get("min_new").and_then(Value::as_u64).unwrap_or(0) as usize,
        };
        serde_json::to_string(&self.manager.select_candidates(query).await?)
            .map_err(|e| AppError::StorageError(e.to_string()))
    }
}

fn required_str<'a>(args: &'a Value, key: &str) -> AppResult<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::InvalidInput(format!("Missing: {key}")))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use rusqlite::Connection as SqliteConnection;

    use super::*;

    struct MockModelCaller {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl NeuronModelCaller for MockModelCaller {
        async fn call_model(&self, _system_prompt: &str, _user_prompt: &str) -> AppResult<String> {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(format!(
                r#"{{"desc":"generated-{call}","content":"content-{call}","weight":1.0,"tool_ids":[]}}"#
            ))
        }
    }

    fn test_manager() -> (Arc<NeuronManager>, std::path::PathBuf) {
        let conn = Arc::new(Mutex::new(SqliteConnection::open_in_memory().unwrap()));
        let store = Arc::new(Mutex::new(NeuronStore::new(conn)));
        store.lock().unwrap().init_table().unwrap();
        let root = std::env::temp_dir().join(format!(
            "agent-app-neuron-manager-{}",
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
            Arc::new(MockModelCaller {
                calls: AtomicUsize::new(0),
            }),
            NeuronConfigReader::new(root.clone()),
        ));
        (manager, root)
    }

    #[tokio::test]
    async fn select_candidates_prefers_source_id_and_fills_to_n() {
        let (manager, root) = test_manager();
        let source = manager
            .create_for_admin(NeuronCreate {
                desc: "source".into(),
                content: "source content".into(),
                ..Default::default()
            })
            .unwrap();
        let candidates = manager
            .select_candidates(CandidateQuery {
                n: 3,
                source_id: Some(source.id.clone()),
                system_type: Some("missing-system-type".into()),
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
    async fn bootstrap_creator_candidates_returns_seven_direct_children() {
        let (manager, root) = test_manager();
        let candidates = manager.bootstrap_creator_candidates().await.unwrap();
        assert_eq!(candidates.len(), 7);
        let creator = manager.ensure_creator_for_admin().unwrap();
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
        let creator = manager.ensure_creator_for_admin().unwrap();
        let result = manager.update_for_ai(
            &creator.id,
            NeuronUpdate {
                desc: Some("changed".into()),
                content: None,
            },
        );
        assert!(result.is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
