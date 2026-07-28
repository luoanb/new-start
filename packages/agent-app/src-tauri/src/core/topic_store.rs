use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};

use super::{
    error::{AppError, AppResult},
    models::{ScopeInItem, Topic, TopicStatus, TopicUpdate},
};

static TOPIC_COUNTER: AtomicU64 = AtomicU64::new(0);

/// TopicStore manages the `topics` table in the shared App-level SQLite database.
pub struct TopicStore {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for TopicStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TopicStore").finish_non_exhaustive()
    }
}

impl TopicStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn init_table(&self) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS topics (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                status      TEXT NOT NULL DEFAULT 'todo',
                description TEXT NOT NULL DEFAULT '',
                scope_in    TEXT NOT NULL DEFAULT '[]',
                progress    INTEGER NOT NULL DEFAULT 0,
                extra       TEXT,
                created_at  INTEGER NOT NULL,
                updated_at  INTEGER NOT NULL
            );",
        )
        .map_err(|e| AppError::StorageError(format!("Failed to init topics table: {}", e)))?;
        Ok(())
    }

    pub fn list(&self, status_filter: Option<TopicStatus>) -> AppResult<Vec<Topic>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;

        let (query, status_str): (&str, Option<String>) = match &status_filter {
            Some(s) => (
                "SELECT id, name, status, description, scope_in, progress, extra, created_at, updated_at FROM topics WHERE status = ?1 ORDER BY created_at DESC",
                Some(status_to_string(s)),
            ),
            None => (
                "SELECT id, name, status, description, scope_in, progress, extra, created_at, updated_at FROM topics ORDER BY created_at DESC",
                None,
            ),
        };

        let mut stmt = conn
            .prepare(query)
            .map_err(|e| AppError::StorageError(format!("Failed to prepare query: {}", e)))?;

        let rows = match status_str {
            Some(ref s) => stmt.query_map(params![s], row_to_topic),
            None => stmt.query_map([], row_to_topic),
        };

        let rows =
            rows.map_err(|e| AppError::StorageError(format!("Failed to query topics: {}", e)))?;

        let mut topics = Vec::new();
        for row in rows {
            topics.push(
                row.map_err(|e| {
                    AppError::StorageError(format!("Failed to read topic row: {}", e))
                })?,
            );
        }
        Ok(topics)
    }

    pub fn get(&self, id: &str) -> AppResult<Option<Topic>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let mut stmt = conn
            .prepare("SELECT id, name, status, description, scope_in, progress, extra, created_at, updated_at FROM topics WHERE id = ?1")
            .map_err(|e| AppError::StorageError(format!("Failed to prepare query: {}", e)))?;

        let mut rows = stmt
            .query_map(params![id], row_to_topic)
            .map_err(|e| AppError::StorageError(format!("Failed to query topic: {}", e)))?;

        match rows.next() {
            Some(Ok(topic)) => Ok(Some(topic)),
            Some(Err(e)) => Err(AppError::StorageError(format!(
                "Failed to read topic row: {}",
                e
            ))),
            None => Ok(None),
        }
    }

    pub fn create(
        &self,
        name: &str,
        description: &str,
        status: TopicStatus,
        scope_in: Vec<ScopeInItem>,
        extra: Option<serde_json::Value>,
    ) -> AppResult<Topic> {
        let now = now_ms();
        let seq = TOPIC_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!("topic_{now}_{seq}");
        let status_str = status_to_string(&status);
        let scope_in_str = serde_json::to_string(&scope_in).unwrap_or_else(|_| "[]".to_string());
        let extra_str = extra
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.execute(
            "INSERT INTO topics (id, name, status, description, scope_in, progress, extra, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?7)",
            params![id, name, status_str, description, scope_in_str, extra_str, now as i64],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to create topic: {}", e)))?;

        Ok(Topic {
            id,
            name: name.to_string(),
            status,
            description: description.to_string(),
            scope_in,
            progress: 0,
            extra,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn update(&self, id: &str, update: TopicUpdate) -> AppResult<Topic> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;

        // Build SET clause dynamically
        let mut set_parts: Vec<String> = Vec::new();
        #[allow(clippy::type_complexity)]
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref name) = update.name {
            set_parts.push("name = ?".to_string());
            param_values.push(Box::new(name.clone()));
        }
        if let Some(ref status) = update.status {
            set_parts.push("status = ?".to_string());
            param_values.push(Box::new(status_to_string(status)));
        }
        if let Some(ref desc) = update.description {
            set_parts.push("description = ?".to_string());
            param_values.push(Box::new(desc.clone()));
        }
        if let Some(ref scope) = update.scope_in {
            let json = serde_json::to_string(scope).unwrap_or_default();
            set_parts.push("scope_in = ?".to_string());
            param_values.push(Box::new(json));
        }
        if let Some(progress) = update.progress {
            set_parts.push("progress = ?".to_string());
            param_values.push(Box::new(progress as i32));
        }
        if let Some(ref extra) = update.extra {
            set_parts.push("extra = ?".to_string());
            let val = extra
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_default());
            param_values.push(Box::new(val));
        }

        if set_parts.is_empty() {
            return self
                .get(id)?
                .ok_or_else(|| AppError::ConversationNotFound(format!("Topic not found: {id}")));
        }

        let now = now_ms();
        set_parts.push("updated_at = ?".to_string());
        param_values.push(Box::new(now as i64));

        let sql = format!("UPDATE topics SET {} WHERE id = ?", set_parts.join(", "));
        param_values.push(Box::new(id.to_string()));

        conn.execute(&sql, rusqlite::params_from_iter(param_values.iter()))
            .map_err(|e| AppError::StorageError(format!("Failed to update topic: {}", e)))?;

        // Fetch updated record
        let mut stmt = conn
            .prepare("SELECT id, name, status, description, scope_in, progress, extra, created_at, updated_at FROM topics WHERE id = ?1")
            .map_err(|e| AppError::StorageError(format!("Failed to prepare query: {}", e)))?;
        let mut rows = stmt
            .query_map(params![id], row_to_topic)
            .map_err(|e| AppError::StorageError(format!("Failed to query topic: {}", e)))?;

        match rows.next() {
            Some(Ok(topic)) => Ok(topic),
            Some(Err(e)) => Err(AppError::StorageError(format!(
                "Failed to read topic row: {}",
                e
            ))),
            None => Err(AppError::ConversationNotFound(format!(
                "Topic not found: {id}"
            ))),
        }
    }

    pub fn delete(&self, id: &str) -> AppResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute("DELETE FROM topics WHERE id = ?1", params![id])
            .map_err(|e| AppError::StorageError(format!("Failed to delete topic: {}", e)))?;
        Ok(affected > 0)
    }
}

fn row_to_topic(row: &rusqlite::Row) -> rusqlite::Result<Topic> {
    let status_str: String = row.get(2)?;
    let scope_in_str: String = row.get(4)?;
    let extra_str: Option<String> = row.get(6)?;

    let status: TopicStatus =
        serde_json::from_str(&format!("\"{}\"", status_str)).unwrap_or(TopicStatus::Todo);
    let scope_in: Vec<ScopeInItem> = serde_json::from_str(&scope_in_str).unwrap_or_default();
    let extra: Option<serde_json::Value> = extra_str.and_then(|s| {
        if s.is_empty() {
            None
        } else {
            serde_json::from_str(&s).ok()
        }
    });

    Ok(Topic {
        id: row.get(0)?,
        name: row.get(1)?,
        status,
        description: row.get(3)?,
        scope_in,
        progress: row.get::<_, i32>(5)? as u8,
        extra,
        created_at: row.get::<_, i64>(7)? as u128,
        updated_at: row.get::<_, i64>(8)? as u128,
    })
}

fn status_to_string(status: &TopicStatus) -> String {
    serde_json::to_string(status)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
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
    use crate::core::models::ScopeInItem;

    fn test_store(name: &str) -> TopicStore {
        let _ = name;
        let conn = Connection::open_in_memory().unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let store = TopicStore::new(conn);
        store.init_table().unwrap();
        store
    }

    #[test]
    fn test_create_and_list() {
        let store = test_store("create_and_list");
        let topic = store
            .create(
                "Test Topic",
                "A description",
                TopicStatus::Todo,
                vec![],
                None,
            )
            .unwrap();
        assert_eq!(topic.name, "Test Topic");
        assert_eq!(topic.status, TopicStatus::Todo);

        let list = store.list(None).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_list_filter_by_status() {
        let store = test_store("filter_by_status");
        store
            .create("Topic 1", "", TopicStatus::Todo, vec![], None)
            .unwrap();
        store
            .create("Topic 2", "", TopicStatus::Done, vec![], None)
            .unwrap();

        let todo_list = store.list(Some(TopicStatus::Todo)).unwrap();
        assert_eq!(todo_list.len(), 1);

        let all = store.list(None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_get_by_id() {
        let store = test_store("get_by_id");
        let created = store
            .create("Get me", "", TopicStatus::Todo, vec![], None)
            .unwrap();
        let fetched = store.get(&created.id).unwrap().expect("should exist");
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.name, "Get me");
    }

    #[test]
    fn test_get_nonexistent() {
        let store = test_store("get_nonexistent");
        let result = store.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_partial() {
        let store = test_store("update_partial");
        let created = store
            .create("Original", "Desc", TopicStatus::Todo, vec![], None)
            .unwrap();

        let updated = store
            .update(
                &created.id,
                TopicUpdate {
                    name: Some("Updated".to_string()),
                    status: Some(TopicStatus::InProgress),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.status, TopicStatus::InProgress);
        assert_eq!(updated.description, "Desc");
        assert_eq!(updated.progress, 0);
    }

    #[test]
    fn test_update_with_scope_in() {
        let store = test_store("update_scope_in");
        let created = store
            .create("Topic", "", TopicStatus::Todo, vec![], None)
            .unwrap();

        let scope = vec![ScopeInItem {
            goal: "Goal 1".to_string(),
            done_contract: "Contract 1".to_string(),
            status: "active".to_string(),
        }];
        let updated = store
            .update(
                &created.id,
                TopicUpdate {
                    scope_in: Some(scope),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.scope_in.len(), 1);
        assert_eq!(updated.scope_in[0].goal, "Goal 1");
    }

    #[test]
    fn test_delete() {
        let store = test_store("delete");
        let created = store
            .create("To delete", "", TopicStatus::Todo, vec![], None)
            .unwrap();

        assert!(store.delete(&created.id).unwrap());
        assert!(store.get(&created.id).unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent() {
        let store = test_store("delete_nonexistent");
        assert!(!store.delete("nonexistent").unwrap());
    }

    #[test]
    fn test_update_progress() {
        let store = test_store("update_progress");
        let created = store
            .create("Progress", "", TopicStatus::InProgress, vec![], None)
            .unwrap();

        let updated = store
            .update(
                &created.id,
                TopicUpdate {
                    progress: Some(50),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.progress, 50);
    }

    #[test]
    fn test_extra_field() {
        let store = test_store("extra_field");
        let extra = Some(serde_json::json!({"key": "value", "count": 42}));

        let created = store
            .create("Extra", "", TopicStatus::Todo, vec![], extra.clone())
            .unwrap();
        assert_eq!(created.extra, extra);

        let fetched = store.get(&created.id).unwrap().unwrap();
        assert_eq!(fetched.extra, extra);

        // Clear extra
        let updated = store
            .update(
                &created.id,
                TopicUpdate {
                    extra: Some(None),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(updated.extra.is_none());
    }
}
