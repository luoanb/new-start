use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};

use super::{
    error::{AppError, AppResult},
    models::{ScopeInItem, Topic, TopicStatus, TopicUpdate},
};

static TOPIC_COUNTER: AtomicU64 = AtomicU64::new(0);
static SCOPE_ITEM_COUNTER: AtomicU64 = AtomicU64::new(0);

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
        migrate_scope_items(&conn)?;
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
        mut scope_in: Vec<ScopeInItem>,
        extra: Option<serde_json::Value>,
    ) -> AppResult<Topic> {
        let now = now_ms();
        let seq = TOPIC_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!("topic_{now}_{seq}");
        if scope_in
            .iter()
            .any(|item| item.goal.trim().is_empty() || item.done_contract.trim().is_empty())
        {
            return Err(AppError::InvalidInput(
                "scope item goal and done_contract cannot be empty".into(),
            ));
        }
        normalize_scope_items(&mut scope_in);
        let (progress, derived_status) = derive_topic_state(&scope_in);
        let status = match status {
            TopicStatus::Paused => TopicStatus::Paused,
            TopicStatus::Cancelled => TopicStatus::Cancelled,
            _ => derived_status,
        };
        let status_str = status_to_string(&status);
        let scope_in_str = serde_json::to_string(&scope_in)
            .map_err(|e| AppError::StorageError(format!("Failed to encode scope_in: {}", e)))?;
        let extra_str = extra
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.execute(
            "INSERT INTO topics (id, name, status, description, scope_in, progress, extra, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![id, name, status_str, description, scope_in_str, progress, extra_str, now as i64],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to create topic: {}", e)))?;

        Ok(Topic {
            id,
            name: name.to_string(),
            status,
            description: description.to_string(),
            scope_in,
            progress,
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
        if let Some(ref desc) = update.description {
            set_parts.push("description = ?".to_string());
            param_values.push(Box::new(desc.clone()));
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

    pub fn add_scope_item(
        &self,
        topic_id: &str,
        goal: &str,
        done_contract: &str,
    ) -> AppResult<Topic> {
        if goal.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "scope item goal cannot be empty".into(),
            ));
        }
        if done_contract.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "scope item done_contract cannot be empty".into(),
            ));
        }
        self.mutate_scope(topic_id, |items| {
            items.push(ScopeInItem {
                id: new_scope_item_id(),
                goal: goal.to_string(),
                done_contract: done_contract.to_string(),
                status: "pending".into(),
            });
            Ok(())
        })
    }

    pub fn delete_scope_item(&self, topic_id: &str, item_id: &str) -> AppResult<Topic> {
        self.mutate_scope(topic_id, |items| {
            let previous_len = items.len();
            items.retain(|item| item.id != item_id);
            if items.len() == previous_len {
                return Err(AppError::InvalidInput(format!(
                    "Scope item not found: {item_id}"
                )));
            }
            Ok(())
        })
    }

    pub fn complete_scope_item(&self, topic_id: &str, item_id: &str) -> AppResult<Topic> {
        self.mutate_scope(topic_id, |items| {
            let item = items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or_else(|| {
                    AppError::InvalidInput(format!("Scope item not found: {item_id}"))
                })?;
            item.status = "completed".into();
            Ok(())
        })
    }

    pub fn pause(&self, id: &str) -> AppResult<Topic> {
        self.set_status(id, TopicStatus::Paused)
    }

    pub fn resume(&self, id: &str) -> AppResult<Topic> {
        let mut topic = self
            .get(id)?
            .ok_or_else(|| AppError::ConversationNotFound(format!("Topic not found: {id}")))?;
        let (progress, status) = derive_topic_state(&topic.scope_in);
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.execute(
            "UPDATE topics SET progress = ?1, status = ?2, updated_at = ?3 WHERE id = ?4",
            params![progress, status_to_string(&status), now as i64, id],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to resume topic: {}", e)))?;
        topic.progress = progress;
        topic.status = status;
        topic.updated_at = now;
        Ok(topic)
    }

    fn mutate_scope<F>(&self, id: &str, mutate: F) -> AppResult<Topic>
    where
        F: FnOnce(&mut Vec<ScopeInItem>) -> AppResult<()>,
    {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let tx = conn
            .transaction()
            .map_err(|e| AppError::StorageError(format!("Failed to start transaction: {}", e)))?;
        let mut topic = tx
            .query_row(
                "SELECT id, name, status, description, scope_in, progress, extra, created_at, updated_at FROM topics WHERE id = ?1",
                params![id],
                row_to_topic,
            )
            .optional()
            .map_err(|e| AppError::StorageError(format!("Failed to query topic: {}", e)))?
            .ok_or_else(|| AppError::ConversationNotFound(format!("Topic not found: {id}")))?;
        if topic.status == TopicStatus::Paused {
            return Err(AppError::InvalidInput(
                "Cannot modify scope_in while topic is paused".into(),
            ));
        }

        mutate(&mut topic.scope_in)?;
        let (progress, status) = derive_topic_state(&topic.scope_in);
        let scope_json = serde_json::to_string(&topic.scope_in)
            .map_err(|e| AppError::StorageError(format!("Failed to encode scope_in: {}", e)))?;
        let now = now_ms();
        tx.execute(
            "UPDATE topics
             SET scope_in = ?1, progress = ?2, status = ?3, updated_at = ?4
             WHERE id = ?5",
            params![
                scope_json,
                progress,
                status_to_string(&status),
                now as i64,
                id
            ],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to update scope_in: {}", e)))?;
        tx.commit()
            .map_err(|e| AppError::StorageError(format!("Failed to commit transaction: {}", e)))?;

        topic.progress = progress;
        topic.status = status;
        topic.updated_at = now;
        Ok(topic)
    }

    fn set_status(&self, id: &str, status: TopicStatus) -> AppResult<Topic> {
        let now = now_ms();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let affected = conn
            .execute(
                "UPDATE topics SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status_to_string(&status), now as i64, id],
            )
            .map_err(|e| AppError::StorageError(format!("Failed to set topic status: {}", e)))?;
        drop(conn);
        if affected == 0 {
            return Err(AppError::ConversationNotFound(format!(
                "Topic not found: {id}"
            )));
        }
        self.get(id)?
            .ok_or_else(|| AppError::ConversationNotFound(format!("Topic not found: {id}")))
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

fn migrate_scope_items(conn: &Connection) -> AppResult<()> {
    let rows = {
        let mut stmt = conn
            .prepare("SELECT id, status, scope_in FROM topics")
            .map_err(|e| AppError::StorageError(format!("Failed to prepare migration: {}", e)))?;
        let mapped = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| AppError::StorageError(format!("Failed to query migration: {}", e)))?;
        let mut rows = Vec::new();
        for row in mapped {
            rows.push(
                row.map_err(|e| AppError::StorageError(format!("Failed to read topic: {}", e)))?,
            );
        }
        rows
    };

    for (id, current_status, scope_json) in rows {
        let mut items: Vec<ScopeInItem> = serde_json::from_str(&scope_json).unwrap_or_default();
        normalize_scope_items(&mut items);
        let (progress, derived_status) = derive_topic_state(&items);
        let status = match current_status.as_str() {
            "paused" => TopicStatus::Paused,
            "cancelled" => TopicStatus::Cancelled,
            _ => derived_status,
        };
        let encoded = serde_json::to_string(&items)
            .map_err(|e| AppError::StorageError(format!("Failed to encode scope_in: {}", e)))?;
        conn.execute(
            "UPDATE topics SET scope_in = ?1, progress = ?2, status = ?3 WHERE id = ?4",
            params![encoded, progress, status_to_string(&status), id],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to migrate scope_in: {}", e)))?;
    }
    Ok(())
}

fn normalize_scope_items(items: &mut [ScopeInItem]) {
    let mut ids = std::collections::HashSet::new();
    for item in items {
        if item.id.trim().is_empty() || !ids.insert(item.id.clone()) {
            item.id = new_scope_item_id();
            ids.insert(item.id.clone());
        }
        item.status = match item.status.as_str() {
            "completed" | "done" => "completed".into(),
            _ => "pending".into(),
        };
    }
}

fn derive_topic_state(items: &[ScopeInItem]) -> (u8, TopicStatus) {
    if items.is_empty() {
        return (0, TopicStatus::Todo);
    }
    let completed = items
        .iter()
        .filter(|item| item.status == "completed")
        .count();
    let progress = ((completed * 100) / items.len()) as u8;
    let status = if completed == 0 {
        TopicStatus::Todo
    } else if completed == items.len() {
        TopicStatus::Done
    } else {
        TopicStatus::InProgress
    };
    (progress, status)
}

fn new_scope_item_id() -> String {
    let now = now_ms();
    let seq = SCOPE_ITEM_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("scope_{now}_{seq}")
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

    fn scope_item(goal: &str, status: &str) -> ScopeInItem {
        ScopeInItem {
            id: String::new(),
            goal: goal.into(),
            done_contract: format!("Finish {goal}"),
            status: status.into(),
        }
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
            .create(
                "Topic 2",
                "",
                TopicStatus::Done,
                vec![scope_item("done", "completed")],
                None,
            )
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
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.status, TopicStatus::Todo);
        assert_eq!(updated.description, "Desc");
        assert_eq!(updated.progress, 0);
    }

    #[test]
    fn test_scope_item_lifecycle_recomputes_topic_state() {
        let store = test_store("scope_item_lifecycle");
        let created = store
            .create("Topic", "", TopicStatus::Todo, vec![], None)
            .unwrap();

        let updated = store
            .add_scope_item(&created.id, "Goal 1", "Contract 1")
            .unwrap();
        assert_eq!(updated.scope_in.len(), 1);
        assert_eq!(updated.scope_in[0].goal, "Goal 1");
        assert!(!updated.scope_in[0].id.is_empty());
        assert_eq!(updated.progress, 0);
        assert_eq!(updated.status, TopicStatus::Todo);

        let item_id = updated.scope_in[0].id.clone();
        let completed = store.complete_scope_item(&created.id, &item_id).unwrap();
        assert_eq!(completed.progress, 100);
        assert_eq!(completed.status, TopicStatus::Done);

        let deleted = store.delete_scope_item(&created.id, &item_id).unwrap();
        assert!(deleted.scope_in.is_empty());
        assert_eq!(deleted.progress, 0);
        assert_eq!(deleted.status, TopicStatus::Todo);
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
    fn test_paused_topic_blocks_scope_mutations_and_resume_derives_status() {
        let store = test_store("pause_scope");
        let created = store
            .create(
                "Progress",
                "",
                TopicStatus::Todo,
                vec![scope_item("goal", "completed")],
                None,
            )
            .unwrap();
        let paused = store.pause(&created.id).unwrap();
        assert_eq!(paused.status, TopicStatus::Paused);
        assert!(store
            .add_scope_item(&created.id, "blocked", "blocked")
            .is_err());
        let resumed = store.resume(&created.id).unwrap();
        assert_eq!(resumed.progress, 100);
        assert_eq!(resumed.status, TopicStatus::Done);
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

    #[test]
    fn test_migrates_legacy_scope_items_and_accepts_large_text() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE topics (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                description TEXT NOT NULL,
                scope_in TEXT NOT NULL,
                progress INTEGER NOT NULL,
                extra TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            INSERT INTO topics VALUES (
                'legacy', 'Legacy', 'in_progress', '',
                '[{\"goal\":\"old\",\"done_contract\":\"old\",\"status\":\"active\"}]',
                99, NULL, 1, 1
            );",
        )
        .unwrap();
        let store = TopicStore::new(Arc::new(Mutex::new(conn)));
        store.init_table().unwrap();
        let migrated = store.get("legacy").unwrap().unwrap();
        assert!(!migrated.scope_in[0].id.is_empty());
        assert_eq!(migrated.scope_in[0].status, "pending");
        assert_eq!(migrated.progress, 0);
        assert_eq!(migrated.status, TopicStatus::Todo);

        let large_text = "长".repeat(50_000);
        let updated = store
            .add_scope_item("legacy", &large_text, &large_text)
            .unwrap();
        assert_eq!(updated.scope_in[1].goal.chars().count(), 50_000);
    }
}
