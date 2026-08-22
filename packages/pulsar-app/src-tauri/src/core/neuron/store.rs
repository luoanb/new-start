use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::{
    error::{AppError, AppResult},
    models::{
        Connection as NeuronConnection, Neuron, NeuronCreate, NeuronKindFilter, NeuronPage,
        NeuronSubgraph, NeuronUpdate, NeuronVariant, NeuronVersion, SessionBehavior,
    },
};

static NEURON_COUNTER: AtomicU64 = AtomicU64::new(0);

/// get_network BFS 结果节点上限（稠密网络防结果撑爆上下文）。
pub const MAX_NETWORK_NODES: usize = 500;

pub struct NeuronStore {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for NeuronStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeuronStore").finish_non_exhaustive()
    }
}

impl NeuronStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn init_table(&self) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS neurons (
                id         TEXT PRIMARY KEY,
                desc       TEXT NOT NULL DEFAULT '',
                content    TEXT NOT NULL DEFAULT '',
                weight     REAL NOT NULL DEFAULT 0.0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS connections (
                source TEXT NOT NULL REFERENCES neurons(id) ON DELETE CASCADE,
                target TEXT NOT NULL REFERENCES neurons(id) ON DELETE CASCADE,
                weight REAL NOT NULL DEFAULT 0.0,
                PRIMARY KEY (source, target)
            );
            PRAGMA foreign_keys = ON;",
        )
        .map_err(|e| AppError::StorageError(format!("Failed to init neuron tables: {}", e)))?;
        if !has_column(&conn, "neurons", "system_type")? {
            conn.execute("ALTER TABLE neurons ADD COLUMN system_type TEXT", [])
                .map_err(|e| AppError::StorageError(format!("Failed to add system_type: {}", e)))?;
        }
        if !has_column(&conn, "neurons", "tool_ids")? {
            conn.execute(
                "ALTER TABLE neurons ADD COLUMN tool_ids TEXT NOT NULL DEFAULT '[]'",
                [],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to add tool_ids: {}", e)))?;
        }
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_neurons_system_type_unique
             ON neurons(system_type) WHERE system_type IS NOT NULL",
            [],
        )
        .map_err(|e| {
            AppError::StorageError(format!("Failed to index neuron system_type: {}", e))
        })?;
        // ── Creator self-iteration columns (nullable/defaulted for legacy rows) ──
        if !has_column(&conn, "neurons", "lineage_parent_id")? {
            conn.execute("ALTER TABLE neurons ADD COLUMN lineage_parent_id TEXT", [])
                .map_err(|e| {
                    AppError::StorageError(format!("Failed to add lineage_parent_id: {}", e))
                })?;
        }
        if !has_column(&conn, "neurons", "use_count")? {
            conn.execute(
                "ALTER TABLE neurons ADD COLUMN use_count INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to add use_count: {}", e)))?;
        }
        if !has_column(&conn, "neurons", "accumulated_delta")? {
            conn.execute(
                "ALTER TABLE neurons ADD COLUMN accumulated_delta REAL NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|e| {
                AppError::StorageError(format!("Failed to add accumulated_delta: {}", e))
            })?;
        }
        if !has_column(&conn, "neurons", "last_used_at")? {
            conn.execute("ALTER TABLE neurons ADD COLUMN last_used_at INTEGER", [])
                .map_err(|e| {
                    AppError::StorageError(format!("Failed to add last_used_at: {}", e))
                })?;
        }
        if !has_column(&conn, "neurons", "variant_state")? {
            conn.execute("ALTER TABLE neurons ADD COLUMN variant_state TEXT", [])
                .map_err(|e| {
                    AppError::StorageError(format!("Failed to add variant_state: {}", e))
                })?;
        }
        if !has_column(&conn, "neurons", "manual_edited")? {
            conn.execute(
                "ALTER TABLE neurons ADD COLUMN manual_edited INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to add manual_edited: {}", e)))?;
        }
        if !has_column(&conn, "neurons", "deleted_at")? {
            conn.execute("ALTER TABLE neurons ADD COLUMN deleted_at INTEGER", [])
                .map_err(|e| AppError::StorageError(format!("Failed to add deleted_at: {}", e)))?;
        }
        if !has_column(&conn, "neurons", "behavior")? {
            conn.execute("ALTER TABLE neurons ADD COLUMN behavior TEXT", [])
                .map_err(|e| AppError::StorageError(format!("Failed to add behavior: {}", e)))?;
        }
        conn.execute(
            "CREATE TABLE IF NOT EXISTS neuron_versions (
                id              TEXT PRIMARY KEY,
                neuron_id       TEXT NOT NULL REFERENCES neurons(id) ON DELETE CASCADE,
                content         TEXT NOT NULL DEFAULT '',
                source          TEXT NOT NULL,
                created_at      INTEGER NOT NULL,
                prev_version_id TEXT
            )",
            [],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to init neuron_versions: {}", e)))?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_neuron_versions_neuron
             ON neuron_versions(neuron_id, created_at DESC)",
            [],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to index neuron_versions: {}", e)))?;
        Ok(())
    }

    pub fn create_neuron(&self, create: NeuronCreate) -> AppResult<Neuron> {
        // Creation always starts at 0; callers may pass weight but it is ignored.
        // Subsequent changes must go through adjust_weight(delta).
        let weight = 0.0;
        if create
            .system_type
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(AppError::InvalidInput("system_type cannot be empty".into()));
        }
        let now = now_ms();
        let seq = NEURON_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!("n_{now}_{seq}");
        let tool_ids = serde_json::to_string(&create.tool_ids)
            .map_err(|e| AppError::StorageError(format!("Failed to encode tool_ids: {}", e)))?;

        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.execute(
            "INSERT INTO neurons
             (id, desc, content, weight, created_at, updated_at, system_type, tool_ids,
              lineage_parent_id, variant_state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                &create.desc,
                &create.content,
                weight,
                now as i64,
                create.system_type.as_deref(),
                &tool_ids,
                create.lineage_parent_id.as_deref(),
                create.variant_state.as_deref()
            ],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to create neuron: {}", e)))?;

        Ok(Neuron {
            id,
            desc: create.desc,
            content: create.content,
            weight,
            system_type: create.system_type,
            tool_ids: create.tool_ids,
            created_at: now,
            updated_at: now,
            use_count: 0,
            last_used_at: None,
            deleted_at: None,
            behavior: None,
        })
    }

    pub fn get_neuron(&self, id: &str) -> AppResult<Option<Neuron>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, desc, content, weight, system_type, tool_ids, created_at, updated_at,
                        use_count, last_used_at, deleted_at, behavior
                 FROM neurons WHERE id = ?1 AND deleted_at IS NULL",
            )
            .map_err(|e| AppError::StorageError(format!("Failed to prepare: {}", e)))?;
        let mut rows = stmt
            .query_map(params![id], row_to_neuron)
            .map_err(|e| AppError::StorageError(format!("Failed to query: {}", e)))?;
        match rows.next() {
            Some(Ok(n)) => Ok(Some(n)),
            Some(Err(e)) => Err(AppError::StorageError(format!("Failed to read: {}", e))),
            None => Ok(None),
        }
    }

    pub fn list_neurons(&self) -> AppResult<Vec<Neuron>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, desc, content, weight, system_type, tool_ids, created_at, updated_at,
                        use_count, last_used_at, deleted_at, behavior
                 FROM neurons WHERE deleted_at IS NULL ORDER BY created_at DESC",
            )
            .map_err(|e| AppError::StorageError(format!("Failed to prepare: {}", e)))?;
        let rows = stmt
            .query_map([], row_to_neuron)
            .map_err(|e| AppError::StorageError(format!("Failed to query: {}", e)))?;
        let mut neurons = Vec::new();
        for row in rows {
            neurons
                .push(row.map_err(|e| AppError::StorageError(format!("Failed to read: {}", e)))?);
        }
        Ok(neurons)
    }

    /// 管理面分页列表：分页 + 搜索（desc/id 模糊）+ 类型筛选（全部/系统/普通）。
    /// LIKE 通配符（`%` / `_` / `\`）按字面转义处理，避免搜索词影响匹配。
    pub fn list_neurons_page(
        &self,
        page: usize,
        page_size: usize,
        search: Option<&str>,
        kind: NeuronKindFilter,
    ) -> AppResult<NeuronPage> {
        let page_size = page_size.clamp(1, 100);
        let offset = page.saturating_mul(page_size);

        let mut conditions: Vec<String> = vec!["deleted_at IS NULL".to_string()];
        match kind {
            NeuronKindFilter::All => {}
            NeuronKindFilter::System => conditions.push("system_type IS NOT NULL".to_string()),
            NeuronKindFilter::Normal => conditions.push("system_type IS NULL".to_string()),
        }
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let trimmed = search.map(str::trim).filter(|s| !s.is_empty());
        if let Some(term) = trimmed {
            let escaped = term
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            conditions.push(
                "(desc LIKE ? ESCAPE '\\' OR id LIKE ? ESCAPE '\\')".to_string(),
            );
            let pattern = format!("%{escaped}%");
            params.push(Box::new(pattern.clone()));
            params.push(Box::new(pattern));
        }
        let where_clause = conditions.join(" AND ");

        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let total: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM neurons WHERE {where_clause}"),
                rusqlite::params_from_iter(params.iter()),
                |row| row.get(0),
            )
            .map_err(|e| AppError::StorageError(format!("Failed to count neurons: {}", e)))?;

        let sql = format!(
            "SELECT id, desc, content, weight, system_type, tool_ids, created_at, updated_at,
                    use_count, last_used_at, deleted_at, behavior
             FROM neurons WHERE {where_clause}
             ORDER BY weight DESC, id ASC LIMIT ? OFFSET ?"
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::StorageError(format!("Failed to prepare: {}", e)))?;
        let mut page_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        page_params.extend(params);
        page_params.push(Box::new(page_size as i64));
        page_params.push(Box::new(offset as i64));
        let rows = stmt
            .query_map(rusqlite::params_from_iter(page_params.iter()), row_to_neuron)
            .map_err(|e| AppError::StorageError(format!("Failed to query: {}", e)))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(
                row.map_err(|e| AppError::StorageError(format!("Failed to read: {}", e)))?,
            );
        }
        drop(stmt);
        drop(conn);
        let total = total as usize;
        Ok(NeuronPage {
            has_more: offset + items.len() < total,
            items,
            total,
        })
    }

    pub fn get_neuron_by_system_type(&self, system_type: &str) -> AppResult<Option<Neuron>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, desc, content, weight, system_type, tool_ids, created_at, updated_at,
                        use_count, last_used_at, deleted_at, behavior
                 FROM neurons WHERE system_type = ?1 AND deleted_at IS NULL",
            )
            .map_err(|e| AppError::StorageError(format!("Failed to prepare: {}", e)))?;
        let mut rows = stmt
            .query_map(params![system_type], row_to_neuron)
            .map_err(|e| AppError::StorageError(format!("Failed to query: {}", e)))?;
        match rows.next() {
            Some(Ok(neuron)) => Ok(Some(neuron)),
            Some(Err(error)) => Err(AppError::StorageError(format!(
                "Failed to read neuron: {}",
                error
            ))),
            None => Ok(None),
        }
    }

    pub fn update_neuron(&self, id: &str, update: NeuronUpdate) -> AppResult<Neuron> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;

        let mut set_parts: Vec<String> = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref d) = update.desc {
            set_parts.push("desc = ?".to_string());
            param_values.push(Box::new(d.clone()));
        }
        if let Some(ref c) = update.content {
            set_parts.push("content = ?".to_string());
            param_values.push(Box::new(c.clone()));
        }
        if let Some(ref t) = update.tool_ids {
            let encoded = serde_json::to_string(t)
                .map_err(|e| AppError::InvalidInput(format!("Failed to encode tool_ids: {}", e)))?;
            set_parts.push("tool_ids = ?".to_string());
            param_values.push(Box::new(encoded));
        }
        if set_parts.is_empty() {
            return Err(AppError::InvalidInput(
                "update_neuron requires at least one field".into(),
            ));
        }

        // 编辑即使用：合并活跃信号（use_count+1, last_used_at）。
        set_parts.push("use_count = use_count + 1".to_string());
        let now = now_ms();
        set_parts.push("last_used_at = ?".to_string());
        param_values.push(Box::new(now as i64));
        set_parts.push("updated_at = ?".to_string());
        param_values.push(Box::new(now as i64));

        let sql = format!(
            "UPDATE neurons SET {} WHERE id = ? AND deleted_at IS NULL",
            set_parts.join(", ")
        );
        param_values.push(Box::new(id.to_string()));

        conn.execute(&sql, rusqlite::params_from_iter(param_values.iter()))
            .map_err(|e| AppError::StorageError(format!("Failed to update neuron: {}", e)))?;

        drop(conn);
        self.get_neuron(id)?
            .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))
    }

    pub fn adjust_weight(&self, id: &str, delta: f64) -> AppResult<Neuron> {
        if !delta.is_finite() {
            return Err(AppError::InvalidInput("weight delta must be finite".into()));
        }
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        // 打分即使用：合并活跃信号（use_count+1, last_used_at）。
        let affected = conn
            .execute(
                "UPDATE neurons SET weight = weight + ?1, use_count = use_count + 1,
                        last_used_at = ?2, updated_at = ?2
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![delta, now as i64, id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to adjust weight: {}", e)))?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NeuronNotFound(id.to_string()));
        }
        self.get_neuron(id)?
            .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))
    }

    pub fn set_system_type(&self, id: &str, system_type: Option<&str>) -> AppResult<Neuron> {
        if system_type.is_some_and(|value| value.trim().is_empty()) {
            return Err(AppError::InvalidInput("system_type cannot be empty".into()));
        }
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "UPDATE neurons SET system_type = ?1, updated_at = ?2
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![system_type, now as i64, id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to set system_type: {}", e)))?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NeuronNotFound(id.to_string()));
        }
        self.get_neuron(id)?
            .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))
    }

    pub fn set_tool_ids(&self, id: &str, tool_ids: Vec<String>) -> AppResult<Neuron> {
        let encoded = serde_json::to_string(&tool_ids)
            .map_err(|e| AppError::StorageError(format!("Failed to encode tool_ids: {}", e)))?;
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "UPDATE neurons SET tool_ids = ?1, updated_at = ?2
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![encoded, now as i64, id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to set tool_ids: {}", e)))?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NeuronNotFound(id.to_string()));
        }
        self.get_neuron(id)?
            .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))
    }

    pub fn delete_neuron(&self, id: &str) -> AppResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute("DELETE FROM neurons WHERE id = ?1", params![id])
            .map_err(|e| AppError::StorageError(format!("Failed to delete neuron: {}", e)))?;
        Ok(affected > 0)
    }

    pub fn create_downstream_neuron(
        &self,
        source_id: &str,
        create: NeuronCreate,
        _edge_weight: f64,
    ) -> AppResult<(Neuron, NeuronConnection)> {
        // Node and edge weights always start at 0; `_edge_weight` is ignored.
        let weight = 0.0;
        let edge_weight = 0.0;
        if create
            .system_type
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(AppError::InvalidInput("system_type cannot be empty".into()));
        }
        let now = now_ms();
        let seq = NEURON_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!("n_{now}_{seq}");
        let tool_ids = serde_json::to_string(&create.tool_ids)
            .map_err(|e| AppError::StorageError(format!("Failed to encode tool_ids: {}", e)))?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let tx = conn
            .transaction()
            .map_err(|e| AppError::StorageError(format!("Failed to start transaction: {}", e)))?;
        tx.execute(
            "INSERT INTO neurons
             (id, desc, content, weight, created_at, updated_at, system_type, tool_ids,
              lineage_parent_id, variant_state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8, ?9)",
            params![
                &id,
                &create.desc,
                &create.content,
                weight,
                now as i64,
                create.system_type.as_deref(),
                &tool_ids,
                create.lineage_parent_id.as_deref(),
                create.variant_state.as_deref()
            ],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to create neuron: {}", e)))?;
        tx.execute(
            "INSERT INTO connections (source, target, weight) VALUES (?1, ?2, ?3)",
            params![source_id, &id, edge_weight],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to link downstream neuron: {}", e)))?;
        tx.commit()
            .map_err(|e| AppError::StorageError(format!("Failed to commit transaction: {}", e)))?;

        let neuron = Neuron {
            id: id.clone(),
            desc: create.desc,
            content: create.content,
            weight,
            system_type: create.system_type,
            tool_ids: create.tool_ids,
            created_at: now,
            updated_at: now,
            use_count: 0,
            last_used_at: None,
            deleted_at: None,
            behavior: None,
        };
        let connection = NeuronConnection {
            source: source_id.to_string(),
            target: id,
            weight: edge_weight,
        };
        Ok((neuron, connection))
    }

    // ── Connection operations ───────────────────────────────────

    pub fn link(&self, source: &str, target: &str, _weight: f64) -> AppResult<NeuronConnection> {
        // New edges always start at 0; change via adjust_connection_weight.
        let weight = 0.0;
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        // 拒绝 deleted 端点：两端都必须是活跃神经元。
        let active_endpoints: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM neurons WHERE id IN (?1, ?2) AND deleted_at IS NULL",
                params![source, target],
                |row| row.get(0),
            )
            .map_err(|e| AppError::StorageError(format!("Failed to check endpoints: {}", e)))?;
        if active_endpoints != 2 {
            return Err(AppError::NeuronNotFound(format!(
                "link endpoint missing or deleted: {source} -> {target}"
            )));
        }
        conn.execute(
            "INSERT OR REPLACE INTO connections (source, target, weight) VALUES (?1, ?2, ?3)",
            params![source, target, weight],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to link: {}", e)))?;
        Ok(NeuronConnection {
            source: source.to_string(),
            target: target.to_string(),
            weight,
        })
    }

    pub fn adjust_connection_weight(
        &self,
        source: &str,
        target: &str,
        delta: f64,
    ) -> AppResult<NeuronConnection> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let existing: Option<f64> = conn
            .query_row(
                "SELECT weight FROM connections WHERE source = ?1 AND target = ?2",
                params![source, target],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AppError::StorageError(format!("Failed to read connection: {}", e)))?;
        let Some(current) = existing else {
            return Err(AppError::InvalidInput(format!(
                "Connection not found: {source} -> {target}"
            )));
        };
        let weight = current + delta;
        conn.execute(
            "UPDATE connections SET weight = ?1 WHERE source = ?2 AND target = ?3",
            params![weight, source, target],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to adjust connection: {}", e)))?;
        Ok(NeuronConnection {
            source: source.to_string(),
            target: target.to_string(),
            weight,
        })
    }

    /// 直接下游存在性检查：`source → target` 边是否存在（回挂规则前置判断）。
    pub fn connection_exists(&self, source: &str, target: &str) -> AppResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM connections WHERE source = ?1 AND target = ?2",
                params![source, target],
                |row| row.get(0),
            )
            .map_err(|e| AppError::StorageError(format!("Failed to check connection: {}", e)))?;
        Ok(count > 0)
    }

    pub fn unlink(&self, source: &str, target: &str) -> AppResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "DELETE FROM connections WHERE source = ?1 AND target = ?2",
                params![source, target],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to unlink: {}", e)))?;
        Ok(affected > 0)
    }

    pub fn unlink_all_edges_of(&self, neuron_id: &str) -> AppResult<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "DELETE FROM connections WHERE source = ?1 OR target = ?1",
                params![neuron_id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to unlink edges: {}", e)))?;
        Ok(affected)
    }

    pub fn get_connections(&self, neuron_id: &str) -> AppResult<Vec<NeuronConnection>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT c.source, c.target, c.weight
                 FROM connections c
                 JOIN neurons src ON src.id = c.source AND src.deleted_at IS NULL
                 JOIN neurons tgt ON tgt.id = c.target AND tgt.deleted_at IS NULL
                 WHERE c.source = ?1 OR c.target = ?1",
            )
            .map_err(|e| AppError::StorageError(format!("Failed to prepare: {}", e)))?;
        let rows = stmt
            .query_map(params![neuron_id], |row| {
                Ok(NeuronConnection {
                    source: row.get(0)?,
                    target: row.get(1)?,
                    weight: row.get(2)?,
                })
            })
            .map_err(|e| AppError::StorageError(format!("Failed to query: {}", e)))?;
        let mut conns = Vec::new();
        for row in rows {
            conns.push(row.map_err(|e| AppError::StorageError(format!("Failed to read: {}", e)))?);
        }
        Ok(conns)
    }

    pub fn list_direct_downstream(
        &self,
        source_id: &str,
        limit: usize,
        excluded_ids: &std::collections::HashSet<String>,
    ) -> AppResult<Vec<Neuron>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT n.id, n.desc, n.content, n.weight, n.system_type, n.tool_ids,
                        n.created_at, n.updated_at, n.use_count, n.last_used_at, n.deleted_at, n.behavior
                 FROM connections c
                 JOIN neurons n ON n.id = c.target
                 WHERE c.source = ?1
                   AND n.deleted_at IS NULL
                   AND (n.variant_state IS NULL OR n.variant_state != 'observing')
                 ORDER BY n.weight DESC, RANDOM()",
            )
            .map_err(|e| AppError::StorageError(format!("Failed to prepare: {}", e)))?;
        let rows = stmt
            .query_map(params![source_id], row_to_neuron)
            .map_err(|e| AppError::StorageError(format!("Failed to query: {}", e)))?;
        let mut neurons = Vec::new();
        for row in rows {
            let neuron =
                row.map_err(|e| AppError::StorageError(format!("Failed to read: {}", e)))?;
            if !excluded_ids.contains(&neuron.id) {
                neurons.push(neuron);
                if neurons.len() == limit {
                    break;
                }
            }
        }
        Ok(neurons)
    }

    // ── Creator variant pool operations ─────────────────────────

    /// List variants downstream of a creator with their usage/score accumulators.
    /// When `active_only` is true, observing variants are excluded
    /// (legacy rows with NULL variant_state are treated as active).
    pub fn get_variants(
        &self,
        creator_id: &str,
        active_only: bool,
    ) -> AppResult<Vec<NeuronVariant>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT n.id, n.desc, n.content, n.weight, n.system_type, n.tool_ids,
                        n.created_at, n.updated_at, n.lineage_parent_id,
                        n.use_count, n.accumulated_delta, n.last_used_at,
                        n.variant_state, n.manual_edited, n.deleted_at, n.behavior
                 FROM connections c
                 JOIN neurons n ON n.id = c.target
                 WHERE c.source = ?1
                   AND n.deleted_at IS NULL
                 ORDER BY n.weight DESC, RANDOM()",
            )
            .map_err(|e| AppError::StorageError(format!("Failed to prepare: {}", e)))?;
        let rows = stmt
            .query_map(params![creator_id], row_to_neuron_variant)
            .map_err(|e| AppError::StorageError(format!("Failed to query: {}", e)))?;
        let mut variants = Vec::new();
        for row in rows {
            let variant =
                row.map_err(|e| AppError::StorageError(format!("Failed to read: {}", e)))?;
            if active_only && variant.variant_state.as_deref() == Some("observing") {
                continue;
            }
            variants.push(variant);
        }
        Ok(variants)
    }

    /// Bump a variant's usage counter and last_used_at.
    pub fn increment_variant_usage(&self, variant_id: &str) -> AppResult<Neuron> {
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "UPDATE neurons
                 SET use_count = use_count + 1, last_used_at = ?1, updated_at = ?1
                 WHERE id = ?2 AND deleted_at IS NULL",
                params![now as i64, variant_id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to bump usage: {}", e)))?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NeuronNotFound(variant_id.to_string()));
        }
        self.get_neuron(variant_id)?
            .ok_or_else(|| AppError::NeuronNotFound(variant_id.to_string()))
    }

    /// Accumulate a signed delta onto a variant's accumulated score.
    pub fn accumulate_variant_delta(&self, variant_id: &str, delta: f64) -> AppResult<Neuron> {
        if !delta.is_finite() {
            return Err(AppError::InvalidInput("delta must be finite".into()));
        }
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "UPDATE neurons
                 SET accumulated_delta = accumulated_delta + ?1, updated_at = ?2
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![delta, now as i64, variant_id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to accumulate delta: {}", e)))?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NeuronNotFound(variant_id.to_string()));
        }
        self.get_neuron(variant_id)?
            .ok_or_else(|| AppError::NeuronNotFound(variant_id.to_string()))
    }

    /// Set a variant's pool state (`active` / `observing`); NULL clears it.
    pub fn set_variant_state(&self, variant_id: &str, state: Option<&str>) -> AppResult<Neuron> {
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "UPDATE neurons
                 SET variant_state = ?1, updated_at = ?2
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![state, now as i64, variant_id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to set variant state: {}", e)))?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NeuronNotFound(variant_id.to_string()));
        }
        self.get_neuron(variant_id)?
            .ok_or_else(|| AppError::NeuronNotFound(variant_id.to_string()))
    }

    /// Mark a neuron as manually edited (locked out of auto-rewrite).
    pub fn set_manual_edited(&self, neuron_id: &str, edited: bool) -> AppResult<Neuron> {
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "UPDATE neurons
                 SET manual_edited = ?1, updated_at = ?2
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![if edited { 1 } else { 0 }, now as i64, neuron_id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to set manual_edited: {}", e)))?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NeuronNotFound(neuron_id.to_string()));
        }
        self.get_neuron(neuron_id)?
            .ok_or_else(|| AppError::NeuronNotFound(neuron_id.to_string()))
    }

    /// Record an immutable version entry (seed / evolve / rollback).
    pub fn insert_neuron_version(
        &self,
        neuron_id: &str,
        content: &str,
        source: &str,
        prev_version_id: Option<&str>,
    ) -> AppResult<NeuronVersion> {
        let now = now_ms();
        let seq = NEURON_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!("v_{now}_{seq}");
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.execute(
            "INSERT INTO neuron_versions
             (id, neuron_id, content, source, created_at, prev_version_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, neuron_id, content, source, now as i64, prev_version_id],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to insert neuron version: {}", e)))?;
        Ok(NeuronVersion {
            id,
            neuron_id: neuron_id.to_string(),
            content: content.to_string(),
            source: source.to_string(),
            created_at: now,
            prev_version_id: prev_version_id.map(String::from),
        })
    }

    /// Lineage parent id of a neuron (the variant/creator that generated it), if any.
    pub fn lineage_parent_id_of(&self, neuron_id: &str) -> AppResult<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        Ok(conn
            .query_row(
                "SELECT lineage_parent_id FROM neurons WHERE id = ?1",
                params![neuron_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| AppError::StorageError(format!("Failed to query lineage: {}", e)))?
            .flatten())
    }

    /// Latest version record of a neuron, if any.
    pub fn latest_version_of(&self, neuron_id: &str) -> AppResult<Option<NeuronVersion>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.query_row(
            "SELECT id, neuron_id, content, source, created_at, prev_version_id
             FROM neuron_versions
             WHERE neuron_id = ?1
             ORDER BY created_at DESC, rowid DESC
             LIMIT 1",
            params![neuron_id],
            |row| {
                Ok(NeuronVersion {
                    id: row.get(0)?,
                    neuron_id: row.get(1)?,
                    content: row.get(2)?,
                    source: row.get(3)?,
                    created_at: row.get::<_, i64>(4)? as u128,
                    prev_version_id: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|e| AppError::StorageError(format!("Failed to query latest version: {}", e)))
    }

    /// Select one direct upstream neuron by node weight; ties are randomized.
    pub fn select_direct_upstream(&self, target_id: &str) -> AppResult<Option<Neuron>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.query_row(
            "SELECT n.id, n.desc, n.content, n.weight, n.system_type, n.tool_ids,
                    n.created_at, n.updated_at, n.use_count, n.last_used_at, n.deleted_at, n.behavior
             FROM connections c
             JOIN neurons n ON n.id = c.source
             WHERE c.target = ?1 AND n.deleted_at IS NULL
             ORDER BY n.weight DESC, RANDOM()
             LIMIT 1",
            params![target_id],
            row_to_neuron,
        )
        .optional()
        .map_err(|e| AppError::StorageError(format!("Failed to query direct upstream: {}", e)))
    }

    pub fn list_global_candidates(
        &self,
        limit: usize,
        excluded_ids: &std::collections::HashSet<String>,
    ) -> AppResult<Vec<Neuron>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, desc, content, weight, system_type, tool_ids, created_at, updated_at,
                        use_count, last_used_at, deleted_at, behavior
                 FROM neurons
                 WHERE deleted_at IS NULL
                   AND system_type IS NULL
                   AND (variant_state IS NULL OR variant_state != 'observing')
                 ORDER BY weight DESC, RANDOM()",
            )
            .map_err(|e| AppError::StorageError(format!("Failed to prepare: {}", e)))?;
        let rows = stmt
            .query_map([], row_to_neuron)
            .map_err(|e| AppError::StorageError(format!("Failed to query: {}", e)))?;
        let mut neurons = Vec::new();
        for row in rows {
            let neuron =
                row.map_err(|e| AppError::StorageError(format!("Failed to read: {}", e)))?;
            if !excluded_ids.contains(&neuron.id) {
                neurons.push(neuron);
                if neurons.len() == limit {
                    break;
                }
            }
        }
        Ok(neurons)
    }

    /// Get ego-network subgraph around a neuron using iterative BFS up to max_depth.
    ///
    /// Neighborhood expansion follows undirected adjacency (in + out edges).
    /// Returned `connections` only include edges whose both endpoints are in `neurons`.
    /// BFS 节点数受 `MAX_NETWORK_NODES` 上限约束（稠密网络防结果撑爆上下文）。
    pub fn get_network(&self, seed_id: &str, max_depth: usize) -> AppResult<NeuronSubgraph> {
        let mut visited = std::collections::HashSet::new();
        let mut neurons: Vec<Neuron> = Vec::new();
        let mut edge_keys = std::collections::HashSet::new();
        let mut connections: Vec<NeuronConnection> = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((seed_id.to_string(), 0usize));

        while let Some((current_id, depth)) = queue.pop_front() {
            if !visited.insert(current_id.clone()) {
                continue;
            }

            // Only add to result if it's a valid neuron (skip errors gracefully)
            if let Some(neuron) = self.get_neuron(&current_id)? {
                neurons.push(neuron);
                if neurons.len() >= MAX_NETWORK_NODES {
                    break;
                }
            }

            if depth >= max_depth {
                continue;
            }

            let conns = self.get_connections(&current_id)?;
            for c in &conns {
                let edge_key = format!("{}->{}", c.source, c.target);
                if edge_keys.insert(edge_key) {
                    connections.push(c.clone());
                }
                let neighbor = if c.source == current_id {
                    c.target.clone()
                } else {
                    c.source.clone()
                };
                if !visited.contains(&neighbor) {
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }

        let neuron_ids: std::collections::HashSet<&str> =
            neurons.iter().map(|n| n.id.as_str()).collect();
        connections.retain(|c| {
            neuron_ids.contains(c.source.as_str()) && neuron_ids.contains(c.target.as_str())
        });

        Ok(NeuronSubgraph {
            seed_id: seed_id.to_string(),
            neurons,
            connections,
        })
    }

    // ── Capacity & low-value recycling (logical delete) ────────

    /// 活跃（未逻辑删除）神经元数量。
    pub fn count_active(&self) -> AppResult<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM neurons WHERE deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(|e| AppError::StorageError(format!("Failed to count neurons: {}", e)))?;
        Ok(count as usize)
    }

    /// 选出 `n` 个最低价值节点 id（系统提示词豁免）。
    /// 低价值排序：weight ASC, use_count ASC, last_used_at ASC (NULL 优先), created_at DESC。
    pub fn select_low_value(&self, n: usize) -> AppResult<Vec<String>> {
        if n == 0 {
            return Ok(Vec::new());
        }
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT id FROM neurons
                 WHERE deleted_at IS NULL AND system_type IS NULL
                 ORDER BY weight ASC, use_count ASC,
                          last_used_at IS NULL DESC, last_used_at ASC,
                          created_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| AppError::StorageError(format!("Failed to prepare: {}", e)))?;
        let rows = stmt
            .query_map(params![n as i64], |row| row.get::<_, String>(0))
            .map_err(|e| AppError::StorageError(format!("Failed to query: {}", e)))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row.map_err(|e| AppError::StorageError(format!("Failed to read: {}", e)))?);
        }
        Ok(ids)
    }

    /// 逻辑删除指定节点：打 `deleted_at` 标记，数据与版本历史保留。
    /// 返回实际被标记的行数（已删除的节点跳过）。
    pub fn mark_deleted(&self, ids: &[String]) -> AppResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "UPDATE neurons SET deleted_at = ?1, updated_at = ?1
             WHERE id IN ({placeholders}) AND deleted_at IS NULL"
        );
        let deleted_at = now as i64;
        let mut params: Vec<&dyn rusqlite::types::ToSql> = vec![&deleted_at];
        params.extend(ids.iter().map(|id| id as &dyn rusqlite::types::ToSql));
        conn.execute(&sql, rusqlite::params_from_iter(params.iter().copied()))
            .map_err(|e| AppError::StorageError(format!("Failed to mark deleted: {}", e)))
    }

    /// 记录一次使用（select_one 命中、手动标记），`use_count+1, last_used_at=now`。
    pub fn mark_used(&self, id: &str) -> AppResult<Neuron> {
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "UPDATE neurons SET use_count = use_count + 1, last_used_at = ?1, updated_at = ?1
                 WHERE id = ?2 AND deleted_at IS NULL",
                params![now as i64, id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to mark used: {}", e)))?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NeuronNotFound(id.to_string()));
        }
        self.get_neuron(id)?
            .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))
    }

    /// 写系统神经元的 behavior（写路径统一收敛到 SessionSpecManager，不触碰 content）。
    pub fn set_behavior(&self, id: &str, behavior: Option<&SessionBehavior>) -> AppResult<Neuron> {
        let encoded = match behavior {
            Some(b) => Some(serde_json::to_string(b).map_err(|e| {
                AppError::StorageError(format!("Failed to encode behavior: {}", e))
            })?),
            None => None,
        };
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "UPDATE neurons SET behavior = ?1, updated_at = ?2
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![encoded, now as i64, id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to set behavior: {}", e)))?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::NeuronNotFound(id.to_string()));
        }
        self.get_neuron(id)?
            .ok_or_else(|| AppError::NeuronNotFound(id.to_string()))
    }
}

/// 宽容解析 behavior 列：缺失 / 空 / 非法 JSON 一律回落 None（旧行兼容）。
fn parse_behavior(raw: Option<String>) -> Option<SessionBehavior> {
    match raw {
        None => None,
        Some(s) if s.trim().is_empty() => None,
        Some(s) => serde_json::from_str(&s).ok(),
    }
}

fn row_to_neuron(row: &rusqlite::Row) -> rusqlite::Result<Neuron> {
    let tool_ids_json: String = row.get(5)?;
    let tool_ids = serde_json::from_str(&tool_ids_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(Neuron {
        id: row.get(0)?,
        desc: row.get(1)?,
        content: row.get(2)?,
        weight: row.get(3)?,
        system_type: row.get(4)?,
        tool_ids,
        created_at: row.get::<_, i64>(6)? as u128,
        updated_at: row.get::<_, i64>(7)? as u128,
        use_count: row.get(8)?,
        last_used_at: row.get::<_, Option<i64>>(9)?.map(|v| v as u128),
        deleted_at: row.get::<_, Option<i64>>(10)?.map(|v| v as u128),
        behavior: parse_behavior(row.get::<_, Option<String>>(11)?),
    })
}

fn row_to_neuron_variant(row: &rusqlite::Row) -> rusqlite::Result<NeuronVariant> {
    let tool_ids_json: String = row.get(5)?;
    let tool_ids = serde_json::from_str(&tool_ids_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(NeuronVariant {
        neuron: Neuron {
            id: row.get(0)?,
            desc: row.get(1)?,
            content: row.get(2)?,
            weight: row.get(3)?,
            system_type: row.get(4)?,
            tool_ids,
            created_at: row.get::<_, i64>(6)? as u128,
            updated_at: row.get::<_, i64>(7)? as u128,
            use_count: row.get(9)?,
            last_used_at: row.get::<_, Option<i64>>(11)?.map(|v| v as u128),
            deleted_at: row.get::<_, Option<i64>>(14)?.map(|v| v as u128),
            behavior: parse_behavior(row.get::<_, Option<String>>(15)?),
        },
        lineage_parent_id: row.get(8)?,
        use_count: row.get(9)?,
        accumulated_delta: row.get(10)?,
        last_used_at: row.get::<_, Option<i64>>(11)?.map(|v| v as u128),
        variant_state: row.get(12)?,
        manual_edited: row.get::<_, i64>(13)? != 0,
    })
}

fn has_column(conn: &Connection, table: &str, column: &str) -> AppResult<bool> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| AppError::StorageError(format!("Failed to inspect table: {}", e)))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| AppError::StorageError(format!("Failed to inspect columns: {}", e)))?;
    for name in names {
        if name.map_err(|e| AppError::StorageError(e.to_string()))? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> NeuronStore {
        let conn = Connection::open_in_memory().unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let store = NeuronStore::new(conn);
        store.init_table().unwrap();
        store
    }

    fn create(store: &NeuronStore, desc: &str, content: &str, weight: f64) -> Neuron {
        store
            .create_neuron(NeuronCreate {
                desc: desc.into(),
                content: content.into(),
                weight,
                ..Default::default()
            })
            .unwrap()
    }

    #[test]
    fn test_create_and_get() {
        let s = test_store();
        let n = create(&s, "test", "hello", 1.0);
        assert_eq!(n.desc, "test");
        assert!((n.weight - 0.0).abs() < f64::EPSILON);
        let got = s.get_neuron(&n.id).unwrap().unwrap();
        assert_eq!(got.content, "hello");
        assert!((got.weight - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_create_and_link_force_zero_weight() {
        let s = test_store();
        let a = create(&s, "A", "", 9.0);
        let b = create(&s, "B", "", 8.0);
        assert!((a.weight - 0.0).abs() < f64::EPSILON);
        assert!((b.weight - 0.0).abs() < f64::EPSILON);

        let link = s.link(&a.id, &b.id, 0.8).unwrap();
        assert!((link.weight - 0.0).abs() < f64::EPSILON);
        let conns = s.get_connections(&a.id).unwrap();
        assert!((conns[0].weight - 0.0).abs() < f64::EPSILON);

        let (child, edge) = s
            .create_downstream_neuron(
                &a.id,
                NeuronCreate {
                    desc: "child".into(),
                    content: "c".into(),
                    weight: 5.0,
                    ..Default::default()
                },
                3.0,
            )
            .unwrap();
        assert!((child.weight - 0.0).abs() < f64::EPSILON);
        assert!((edge.weight - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_list_neurons() {
        let s = test_store();
        create(&s, "a", "", 0.0);
        create(&s, "b", "", 0.0);
        assert_eq!(s.list_neurons().unwrap().len(), 2);
    }

    #[test]
    fn test_update_neuron() {
        let s = test_store();
        let n = create(&s, "old", "", 0.0);
        let u = s
            .update_neuron(
                &n.id,
                NeuronUpdate {
                    desc: Some("new".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(u.desc, "new");
        let u = s.adjust_weight(&n.id, 5.5).unwrap();
        assert!((u.weight - 5.5).abs() < 1e-6);
    }

    #[test]
    fn test_delete_neuron() {
        let s = test_store();
        let n = create(&s, "del", "", 0.0);
        assert!(s.delete_neuron(&n.id).unwrap());
        assert!(s.get_neuron(&n.id).unwrap().is_none());
    }

    #[test]
    fn test_link_and_get_connections() {
        let s = test_store();
        let a = create(&s, "A", "", 0.0);
        let b = create(&s, "B", "", 0.0);
        s.link(&a.id, &b.id, 0.8).unwrap();

        let conns = s.get_connections(&a.id).unwrap();
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].target, b.id);
    }

    #[test]
    fn select_direct_upstream_prefers_highest_node_weight_and_randomizes_ties() {
        let s = test_store();
        let low = create(&s, "low", "", 0.0);
        let high_a = create(&s, "high-a", "", 0.0);
        let high_b = create(&s, "high-b", "", 0.0);
        let child = create(&s, "child", "", 0.0);
        s.adjust_weight(&low.id, 1.0).unwrap();
        s.adjust_weight(&high_a.id, 5.0).unwrap();
        s.adjust_weight(&high_b.id, 5.0).unwrap();
        s.link(&low.id, &child.id, 0.0).unwrap();
        s.link(&high_a.id, &child.id, 0.0).unwrap();
        s.link(&high_b.id, &child.id, 0.0).unwrap();

        for _ in 0..20 {
            let selected = s.select_direct_upstream(&child.id).unwrap().unwrap();
            assert!(selected.id == high_a.id || selected.id == high_b.id);
        }
        assert!(s.select_direct_upstream(&low.id).unwrap().is_none());
    }

    #[test]
    fn test_unlink() {
        let s = test_store();
        let a = create(&s, "A", "", 0.0);
        let b = create(&s, "B", "", 0.0);
        s.link(&a.id, &b.id, 1.0).unwrap();
        assert!(s.unlink(&a.id, &b.id).unwrap());
        assert_eq!(s.get_connections(&a.id).unwrap().len(), 0);
    }

    #[test]
    fn test_network_bfs() {
        let s = test_store();
        let a = create(&s, "A", "", 0.0);
        let b = create(&s, "B", "", 0.0);
        let c = create(&s, "C", "", 0.0);
        let d = create(&s, "D", "", 0.0);

        s.link(&a.id, &b.id, 1.0).unwrap();
        s.link(&b.id, &c.id, 1.0).unwrap();
        s.link(&c.id, &d.id, 1.0).unwrap();

        // depth 1 from A → A, B (2); edge A→B
        let net = s.get_network(&a.id, 1).unwrap();
        assert_eq!(net.seed_id, a.id);
        assert_eq!(net.neurons.len(), 2);
        assert_eq!(net.connections.len(), 1);
        assert_eq!(net.connections[0].source, a.id);
        assert_eq!(net.connections[0].target, b.id);

        // depth 2 from A → A, B, C (3); edges A→B, B→C
        let net = s.get_network(&a.id, 2).unwrap();
        assert_eq!(net.neurons.len(), 3);
        assert_eq!(net.connections.len(), 2);

        // depth 3 from A → all 4; edges A→B, B→C, C→D
        let net = s.get_network(&a.id, 3).unwrap();
        assert_eq!(net.neurons.len(), 4);
        assert_eq!(net.connections.len(), 3);
    }

    #[test]
    fn test_migrates_legacy_neuron_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE neurons (
                id TEXT PRIMARY KEY,
                desc TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                weight REAL NOT NULL DEFAULT 0.0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )
        .unwrap();
        let store = NeuronStore::new(Arc::new(Mutex::new(conn)));
        store.init_table().unwrap();
        let neuron = create(&store, "migrated", "content", 0.0);
        assert_eq!(neuron.system_type, None);
        assert!(neuron.tool_ids.is_empty());
    }

    #[test]
    fn test_system_type_is_unique_and_downstream_creation_is_atomic() {
        let store = test_store();
        let first = store
            .create_neuron(NeuronCreate {
                desc: "system".into(),
                content: "prompt".into(),
                system_type: Some("topic_match".into()),
                ..Default::default()
            })
            .unwrap();
        let duplicate = store.create_neuron(NeuronCreate {
            desc: "duplicate".into(),
            content: "prompt".into(),
            system_type: Some("topic_match".into()),
            ..Default::default()
        });
        assert!(duplicate.is_err());

        let (child, connection) = store
            .create_downstream_neuron(
                &first.id,
                NeuronCreate {
                    desc: "child".into(),
                    content: "content".into(),
                    weight: 4.0,
                    ..Default::default()
                },
                1.0,
            )
            .unwrap();
        assert_eq!(connection.source, first.id);
        assert_eq!(connection.target, child.id);
        assert!((child.weight - 0.0).abs() < f64::EPSILON);
        assert!((connection.weight - 0.0).abs() < f64::EPSILON);
        let downstream = store
            .list_direct_downstream(&first.id, 10, &std::collections::HashSet::new())
            .unwrap();
        assert_eq!(downstream.len(), 1);
        assert_eq!(downstream[0].id, child.id);

        let count_before = store.list_neurons().unwrap().len();
        let failed = store.create_downstream_neuron(
            "missing-source",
            NeuronCreate {
                desc: "orphan".into(),
                content: "content".into(),
                ..Default::default()
            },
            1.0,
        );
        assert!(failed.is_err());
        assert_eq!(store.list_neurons().unwrap().len(), count_before);
    }

    // ── Capacity & low-value recycling ─────────────────────

    #[test]
    fn test_recycle_low_value_ordering() {
        let s = test_store();
        // a: weight 0, use 0（最低价值）；b: use 1；c: weight 2。
        let a = create(&s, "a", "", 0.0);
        let b = create(&s, "b", "", 0.0);
        let c = create(&s, "c", "", 0.0);
        s.mark_used(&b.id).unwrap();
        s.adjust_weight(&c.id, 2.0).unwrap();

        let victims = s.select_low_value(2).unwrap();
        assert_eq!(victims, vec![a.id.clone(), b.id.clone()]);

        let one = s.select_low_value(1).unwrap();
        assert_eq!(one, vec![a.id]);
    }

    #[test]
    fn test_recycle_exempts_system_neurons() {
        let s = test_store();
        let plain = create(&s, "plain", "", 0.0);
        let sys = s
            .create_neuron(NeuronCreate {
                desc: "sys".into(),
                content: "prompt".into(),
                system_type: Some("selector".into()),
                ..Default::default()
            })
            .unwrap();

        let victims = s.select_low_value(10).unwrap();
        assert!(victims.contains(&plain.id));
        assert!(!victims.contains(&sys.id));
    }

    #[test]
    fn test_recycle_deleted_nodes_excluded_everywhere() {
        let s = test_store();
        let a = create(&s, "a", "", 0.0);
        let b = create(&s, "b", "", 0.0);
        let c = create(&s, "c", "", 0.0);
        s.link(&a.id, &b.id, 0.0).unwrap();

        assert_eq!(s.mark_deleted(&[c.id.clone()]).unwrap(), 1);
        assert_eq!(s.count_active().unwrap(), 2);
        assert!(s.get_neuron(&c.id).unwrap().is_none());
        assert_eq!(s.list_neurons().unwrap().len(), 2);
        assert!(!s
            .list_global_candidates(10, &std::collections::HashSet::new())
            .unwrap()
            .iter()
            .any(|n| n.id == c.id));
        // 重复删除幂等。
        assert_eq!(s.mark_deleted(&[c.id]).unwrap(), 0);
    }

    #[test]
    fn test_list_global_candidates_excludes_system_and_observing() {
        let s = test_store();
        let plain = create(&s, "plain", "", 0.0);
        s.adjust_weight(&plain.id, 9.0).unwrap();
        let sys = s
            .create_neuron(NeuronCreate {
                desc: "sys".into(),
                content: "prompt".into(),
                system_type: Some("selector".into()),
                ..Default::default()
            })
            .unwrap();
        let observing = create(&s, "observing", "", 0.0);
        s.set_variant_state(&observing.id, Some("observing"))
            .unwrap();

        let candidates = s
            .list_global_candidates(10, &std::collections::HashSet::new())
            .unwrap();
        let ids: std::collections::HashSet<&str> =
            candidates.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(plain.id.as_str()));
        assert!(!ids.contains(sys.id.as_str()));
        assert!(!ids.contains(observing.id.as_str()));
    }

    #[test]
    fn test_recycle_deleted_blocks_writes_and_links() {
        let s = test_store();
        let a = create(&s, "a", "", 0.0);
        let b = create(&s, "b", "", 0.0);
        let s1 = create(&s, "s1", "", 0.0);
        s.mark_deleted(&[a.id.clone()]).unwrap();

        assert!(s.adjust_weight(&a.id, 1.0).is_err());
        assert!(s
            .update_neuron(
                &a.id,
                NeuronUpdate {
                    desc: Some("x".into()),
                    ..Default::default()
                },
            )
            .is_err());
        assert!(s.mark_used(&a.id).is_err());
        // 连到已删除端点被拒绝。
        assert!(s.link(&a.id, &b.id, 0.0).is_err());
        assert!(s.link(&b.id, &a.id, 0.0).is_err());
        assert!(s.link(&b.id, &s1.id, 0.0).is_ok());
    }
}
