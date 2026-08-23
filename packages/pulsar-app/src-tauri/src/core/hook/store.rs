//! 裁决记录存储（`hook_judgements` 表）：hook 级裁决调用的全量账本。
//!
//! 与 `topic_store` 同构（`conn: Arc<Mutex<Connection>>` + `on_change: StateEmitter` +
//! `init_table` + 统一 `emit_change`）。裁决调用走两阶段写入：
//! 开始（`insert_start`，status=pending，广播 pending 事件）→ 结束（`finish`，
//! 收敛为终态，广播终态事件）。前端据此就地渲染「裁决中」卡并原地收敛，不重拉全量。
//!
//! 只读账本：无更新/删除/重跑命令；全量保留（`raw_response` / `attempts_detail` 原文不截断）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};

use serde::{Deserialize, Serialize};

use crate::core::{
    error::{AppError, AppResult},
    events::{StateChange, StateEmitter},
};

static HOOK_JUDGEMENT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 裁决记录（列表/详情共用，JSON 序列化直出前端）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookJudgementRecord {
    pub id: String,
    pub session_id: Option<String>,
    pub conversation_id: String,
    /// 锚点消息索引（消息列表裁决卡挂载位置）；未绑定消息为 None。
    pub anchor_message_index: Option<i64>,
    /// system_type（如 `assistant_complete_scope`）。
    pub hook_type: String,
    /// 挂载注入点（如 `after_load_context` / `after_persist_outcome`）；旧记录为 NULL。
    pub inject_point: Option<String>,
    /// `pending` / `ok` / `retried_ok` / `downgraded`。
    pub status: String,
    /// 尝试次数（1 或 2）。
    pub attempts: i64,
    /// 每轮尝试明细（JSON 数组：`[{attempt, raw, error}]`，重试两轮原文全量保留）。
    pub attempts_detail: String,
    /// 用户侧裁决输入（JSON 序列化）。
    pub payload: String,
    /// 最终轮模型原始输出（全文保留）。
    pub raw_response: String,
    /// 解析出的 JSON 决策（成功时；降级时为空串）。
    pub decision: Option<String>,
    /// 失败/降级原因摘要（如 `LLM response missing JSON object`）。
    pub error: Option<String>,
    /// 总耗时（含重试），毫秒。
    pub duration_ms: i64,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    /// 开始时间戳（ms）。
    pub created_at: i64,
    /// 结束时间戳（ms）。
    pub updated_at: i64,
}

/// 列表查询过滤（command `hook_judgements_list` 入参，camelCase 与前端一致）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookJudgementFilter {
    pub hook_type: Option<String>,
    pub status: Option<String>,
    pub conversation_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// 分页列表出参：记录 + 过滤后总数（供前端计数与 hasMore 判断）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookJudgementListResult {
    pub records: Vec<HookJudgementRecord>,
    pub total: i64,
}

/// 构建过滤 WHERE 片段与参数（`hook_type` / `status` / `conversation_id`；
/// `limit` / `offset` 由调用方按需追加）。`list` 与 `list_with_total` 共用，避免逻辑漂移。
fn build_where(
    filter: &HookJudgementFilter,
) -> (Vec<String>, Vec<Box<dyn rusqlite::types::ToSql>>) {
    let mut where_clauses: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    if let Some(ref hook_type) = filter.hook_type {
        where_clauses.push("hook_type = ?".to_string());
        param_values.push(Box::new(hook_type.clone()));
    }
    if let Some(ref status) = filter.status {
        where_clauses.push("status = ?".to_string());
        param_values.push(Box::new(status.clone()));
    }
    if let Some(ref conversation_id) = filter.conversation_id {
        where_clauses.push("conversation_id = ?".to_string());
        param_values.push(Box::new(conversation_id.clone()));
    }
    (where_clauses, param_values)
}

/// HookJudgementStore manages the `hook_judgements` table in the shared App-level SQLite database.
pub struct HookJudgementStore {
    conn: Arc<Mutex<Connection>>,
    /// 变更通知：裁决开始/结束两阶段广播，供前端实时刷新裁决卡与面板。
    on_change: Option<StateEmitter>,
}

impl std::fmt::Debug for HookJudgementStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookJudgementStore").finish_non_exhaustive()
    }
}

impl HookJudgementStore {
    pub fn new(conn: Arc<Mutex<Connection>>, on_change: Option<StateEmitter>) -> Self {
        Self { conn, on_change }
    }

    /// 裁决变更统一广播（两阶段，锚点驱动）：开始 = pending，结束 = 终态。
    fn emit_change(&self, record: &HookJudgementRecord) {
        if let Some(emit) = self.on_change.as_ref() {
            emit(StateChange::HookJudgements {
                conversation_id: record.conversation_id.clone(),
                anchor_message_index: record.anchor_message_index,
                id: record.id.clone(),
                status: record.status.clone(),
            });
        }
    }

    pub fn init_table(&self) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS hook_judgements (
                id                   TEXT PRIMARY KEY,
                session_id           TEXT,
                conversation_id      TEXT NOT NULL,
                anchor_message_index INTEGER,
                hook_type            TEXT NOT NULL,
                inject_point         TEXT,
                status               TEXT NOT NULL DEFAULT 'pending',
                attempts             INTEGER NOT NULL DEFAULT 0,
                attempts_detail      TEXT NOT NULL DEFAULT '[]',
                payload              TEXT NOT NULL DEFAULT '{}',
                raw_response         TEXT NOT NULL DEFAULT '',
                decision             TEXT,
                error                TEXT,
                duration_ms          INTEGER NOT NULL DEFAULT 0,
                model_provider       TEXT,
                model_id             TEXT,
                created_at           INTEGER NOT NULL,
                updated_at           INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_hook_judgements_anchor
                ON hook_judgements(conversation_id, anchor_message_index);
            CREATE INDEX IF NOT EXISTS idx_hook_judgements_hook_status
                ON hook_judgements(hook_type, status);
            CREATE INDEX IF NOT EXISTS idx_hook_judgements_created
                ON hook_judgements(created_at);",
        )
        .map_err(|e| AppError::StorageError(format!("Failed to init hook_judgements table: {}", e)))?;
        // 迁移：既有库补 inject_point 列（NULL 兼容，不重建表；重复执行幂等——已存在时报
        // "duplicate column name"，静默通过）。
        match conn.execute("ALTER TABLE hook_judgements ADD COLUMN inject_point TEXT", []) {
            Ok(_) => Ok(()),
            Err(e) if e.to_string().contains("duplicate column name") => Ok(()),
            Err(e) => Err(AppError::StorageError(format!(
                "Failed to add inject_point column: {}",
                e
            ))),
        }?;
        Ok(())
    }

    /// 两阶段写入 · 开始：插入 pending 记录并广播开始事件（前端就地渲染「裁决中」卡）。
    pub fn insert_start(
        &self,
        id: &str,
        conversation_id: &str,
        session_id: Option<&str>,
        anchor_message_index: Option<i64>,
        hook_type: &str,
        inject_point: Option<&str>,
        payload: &serde_json::Value,
        model_provider: Option<&str>,
        model_id: Option<&str>,
    ) -> AppResult<HookJudgementRecord> {
        let now = now_ms();
        let payload_str = payload.to_string();
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        conn.execute(
            "INSERT INTO hook_judgements
                (id, session_id, conversation_id, anchor_message_index, hook_type, inject_point,
                 status, attempts, attempts_detail, payload, raw_response, decision, error,
                 duration_ms, model_provider, model_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', 0, '[]', ?7, '', NULL, NULL, 0, ?8, ?9,
                     ?10, ?10)",
            params![
                id,
                session_id,
                conversation_id,
                anchor_message_index,
                hook_type,
                inject_point,
                payload_str,
                model_provider,
                model_id,
                now,
            ],
        )
        .map_err(|e| AppError::StorageError(format!("Failed to insert hook judgement: {}", e)))?;
        let record = HookJudgementRecord {
            id: id.to_string(),
            session_id: session_id.map(String::from),
            conversation_id: conversation_id.to_string(),
            anchor_message_index,
            hook_type: hook_type.to_string(),
            inject_point: inject_point.map(String::from),
            status: "pending".into(),
            attempts: 0,
            attempts_detail: "[]".into(),
            payload: payload_str,
            raw_response: String::new(),
            decision: None,
            error: None,
            duration_ms: 0,
            model_provider: model_provider.map(String::from),
            model_id: model_id.map(String::from),
            created_at: now,
            updated_at: now,
        };
        self.emit_change(&record);
        Ok(record)
    }

    /// 两阶段写入 · 结束：收敛终态（status/decision/raw/attempts/error/duration）并广播终态事件。
    pub fn finish(
        &self,
        id: &str,
        status: &str,
        attempts: i64,
        attempts_detail: &[serde_json::Value],
        raw_response: &str,
        decision: Option<&serde_json::Value>,
        error: Option<&str>,
        duration_ms: u64,
    ) -> AppResult<HookJudgementRecord> {
        let now = now_ms();
        let attempts_detail_str = serde_json::to_string(attempts_detail)
            .map_err(|e| AppError::StorageError(format!("Failed to encode attempts_detail: {}", e)))?;
        let decision_str = decision.map(|v| v.to_string());
        // 写锁作用域限定在此块内：执行 UPDATE 后立即释放 conn guard，
        // 避免随后 `self.get(id)` 在同一线程重复加锁造成死锁。
        {
            let conn = self
                .conn
                .lock()
                .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
            conn.execute(
                "UPDATE hook_judgements SET
                    status = ?1, attempts = ?2, attempts_detail = ?3, raw_response = ?4,
                    decision = ?5, error = ?6, duration_ms = ?7, updated_at = ?8
                 WHERE id = ?9",
                params![
                    status,
                    attempts,
                    attempts_detail_str,
                    raw_response,
                    decision_str,
                    error,
                    duration_ms as i64,
                    now,
                    id,
                ],
            )
            .map_err(|e| {
                AppError::StorageError(format!("Failed to finish hook judgement: {}", e))
            })?;
        }
        let record = self.get(id)?.ok_or_else(|| {
            AppError::StorageError(format!("Hook judgement not found after finish: {id}"))
        })?;
        self.emit_change(&record);
        Ok(record)
    }

    pub fn get(&self, id: &str) -> AppResult<Option<HookJudgementRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;
        let mut stmt = conn
            .prepare(&format!("{SELECT_COLUMNS} WHERE id = ?1"))
            .map_err(|e| AppError::StorageError(format!("Failed to prepare query: {}", e)))?;
        let mut rows = stmt
            .query_map(params![id], row_to_record)
            .map_err(|e| AppError::StorageError(format!("Failed to query hook judgement: {}", e)))?;
        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            Some(Err(e)) => Err(AppError::StorageError(format!(
                "Failed to read hook judgement row: {}",
                e
            ))),
            None => Ok(None),
        }
    }

    /// 列表查询（按 `created_at` 倒序）。空过滤 = 全量。兼容既有消费方（无总数）。
    pub fn list(&self, filter: &HookJudgementFilter) -> AppResult<Vec<HookJudgementRecord>> {
        Ok(self.list_with_total(filter)?.records)
    }

    /// 分页列表查询（按 `created_at` 倒序）：单锁内先 `COUNT(*)`（同过滤）再分页 `SELECT`，
    /// 返回记录与过滤后总数。供面板分页与计数消费。
    pub fn list_with_total(
        &self,
        filter: &HookJudgementFilter,
    ) -> AppResult<HookJudgementListResult> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| AppError::StorageError(format!("Failed to lock database: {}", e)))?;

        let (where_clauses, filter_params) = build_where(filter);

        // COUNT（同过滤，不含 limit/offset）。
        let mut count_sql = String::from("SELECT COUNT(*) FROM hook_judgements");
        if !where_clauses.is_empty() {
            count_sql.push_str(" WHERE ");
            count_sql.push_str(&where_clauses.join(" AND "));
        }
        let mut count_stmt = conn
            .prepare(&count_sql)
            .map_err(|e| AppError::StorageError(format!("Failed to prepare count query: {}", e)))?;
        let total: i64 = count_stmt
            .query_row(rusqlite::params_from_iter(filter_params.iter()), |r| r.get(0))
            .map_err(|e| AppError::StorageError(format!("Failed to count hook judgements: {}", e)))?;

        // 分页 SELECT。
        let mut sql = String::from(SELECT_COLUMNS);
        if !where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at DESC");
        // LIMIT/OFFSET 组合拼接：单独 OFFSET 时用 `LIMIT -1 OFFSET ?`（SQLite 无限制占位）。
        let mut sel_params: Vec<Box<dyn rusqlite::types::ToSql>> = filter_params;
        match (filter.limit, filter.offset) {
            (Some(limit), Some(offset)) => {
                sql.push_str(" LIMIT ? OFFSET ?");
                sel_params.push(Box::new(limit));
                sel_params.push(Box::new(offset));
            }
            (Some(limit), None) => {
                sql.push_str(" LIMIT ?");
                sel_params.push(Box::new(limit));
            }
            (None, Some(offset)) => {
                sql.push_str(" LIMIT -1 OFFSET ?");
                sel_params.push(Box::new(offset));
            }
            (None, None) => {}
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::StorageError(format!("Failed to prepare query: {}", e)))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(sel_params.iter()), row_to_record)
            .map_err(|e| AppError::StorageError(format!("Failed to query hook judgements: {}", e)))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(
                row.map_err(|e| AppError::StorageError(format!("Failed to read hook judgement row: {}", e)))?,
            );
        }
        Ok(HookJudgementListResult { records, total })
    }
}

const SELECT_COLUMNS: &str = "SELECT id, session_id, conversation_id, anchor_message_index, hook_type, status, attempts, attempts_detail, payload, raw_response, decision, error, duration_ms, model_provider, model_id, created_at, updated_at, inject_point FROM hook_judgements";

fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<HookJudgementRecord> {
    Ok(HookJudgementRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        conversation_id: row.get(2)?,
        anchor_message_index: row.get(3)?,
        hook_type: row.get(4)?,
        status: row.get(5)?,
        attempts: row.get(6)?,
        attempts_detail: row.get(7)?,
        payload: row.get(8)?,
        raw_response: row.get(9)?,
        decision: row.get(10)?,
        error: row.get(11)?,
        duration_ms: row.get(12)?,
        model_provider: row.get(13)?,
        model_id: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        inject_point: row.get(17)?,
    })
}

pub fn new_hook_judgement_id() -> String {
    let now = now_ms();
    let seq = HOOK_JUDGEMENT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("hj_{now}_{seq}")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store(name: &str) -> HookJudgementStore {
        let _ = name;
        let conn = Connection::open_in_memory().unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let store = HookJudgementStore::new(conn, None);
        store.init_table().unwrap();
        store
    }

    #[test]
    fn test_insert_start_and_finish() {
        let store = test_store("insert_finish");
        let id = new_hook_judgement_id();
        store
            .insert_start(
                &id,
                "conv_1",
                Some("sess_1"),
                Some(3),
                "assistant_complete_scope",
                Some("after_persist_outcome"),
                &serde_json::json!({"topic_id": "t1"}),
                Some("openai"),
                Some("gpt-4o"),
            )
            .unwrap();

        let pending = store.get(&id).unwrap().unwrap();
        assert_eq!(pending.status, "pending");
        assert_eq!(pending.conversation_id, "conv_1");
        assert_eq!(pending.anchor_message_index, Some(3));
        assert_eq!(pending.inject_point.as_deref(), Some("after_persist_outcome"));

        store
            .finish(
                &id,
                "ok",
                1,
                &[serde_json::json!({"attempt": 1, "raw": "{}", "error": null})],
                "{}",
                Some(&serde_json::json!({"completed_item_ids": []})),
                None,
                123,
            )
            .unwrap();
        let done = store.get(&id).unwrap().unwrap();
        assert_eq!(done.status, "ok");
        assert_eq!(done.attempts, 1);
        assert_eq!(done.duration_ms, 123);
        assert!(done.decision.unwrap().contains("completed_item_ids"));
    }

    #[test]
    fn test_finish_downgraded_keeps_raw() {
        let store = test_store("finish_downgraded");
        let id = new_hook_judgement_id();
        store
            .insert_start(
                &id,
                "conv_1",
                None,
                None,
                "assistant_match_topic",
                Some("after_load_context"),
                &serde_json::json!({"user_input": "hi"}),
                None,
                None,
            )
            .unwrap();
        let raw = "I think the answer is yes because...".to_string();
        store
            .finish(
                &id,
                "downgraded",
                2,
                &[
                    serde_json::json!({"attempt": 1, "raw": raw, "error": "missing JSON object"}),
                    serde_json::json!({"attempt": 2, "raw": raw.clone(), "error": "missing JSON object"}),
                ],
                &raw,
                None,
                Some("LLM response missing JSON object"),
                456,
            )
            .unwrap();
        let done = store.get(&id).unwrap().unwrap();
        assert_eq!(done.status, "downgraded");
        assert_eq!(done.raw_response, raw);
        assert_eq!(done.attempts, 2);
        assert!(done.error.unwrap().contains("missing JSON"));
    }

    #[test]
    fn test_list_filters_and_order() {
        let store = test_store("list_filters");
        for i in 0..3 {
            let id = new_hook_judgement_id();
            store
                .insert_start(
                    &id,
                    &format!("conv_{}", i % 2),
                    None,
                    Some(i),
                    if i == 0 { "assistant_complete_scope" } else { "assistant_match_topic" },
                    None, // 兼容：inject_point 可缺省（旧语义）
                    &serde_json::json!({"n": i}),
                    None,
                    None,
                )
                .unwrap();
        }
        // 全量倒序（created_at 相同，id 序保证只验数量）
        let all = store.list(&HookJudgementFilter::default()).unwrap();
        assert_eq!(all.len(), 3);

        let matched = store
            .list(&HookJudgementFilter {
                hook_type: Some("assistant_match_topic".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(matched.len(), 2);

        let conv0 = store
            .list(&HookJudgementFilter {
                conversation_id: Some("conv_0".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(conv0.len(), 2);

        let limited = store
            .list(&HookJudgementFilter {
                limit: Some(1),
                offset: None,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(limited.len(), 1);

        let pending = store
            .list(&HookJudgementFilter {
                status: Some("pending".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[test]
    fn test_list_with_total_pagination() {
        let store = test_store("list_with_total");
        // 插 7 条 pending：conv_0 3 条、conv_1 4 条。
        for i in 0..7 {
            let id = new_hook_judgement_id();
            store
                .insert_start(
                    &id,
                    &format!("conv_{}", i % 2),
                    None,
                    Some(i),
                    "assistant_match_topic",
                    Some("after_load_context"),
                    &serde_json::json!({"n": i}),
                    None,
                    None,
                )
                .unwrap();
        }
        // 取时间线倒序前 2 条收敛为 ok 终态，验证状态过滤下的 total。
        let first_two = store
            .list(&HookJudgementFilter {
                limit: Some(2),
                offset: None,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(first_two.len(), 2);
        for rec in &first_two {
            store
                .finish(
                    &rec.id,
                    "ok",
                    1,
                    &[],
                    "{}",
                    Some(&serde_json::json!({})),
                    None,
                    10,
                )
                .unwrap();
        }

        // 全量：total=7 且 records 全量。
        let all = store
            .list_with_total(&HookJudgementFilter::default())
            .unwrap();
        assert_eq!(all.total, 7);
        assert_eq!(all.records.len(), 7);

        // 状态过滤：status=ok → total=2。
        let ok = store
            .list_with_total(&HookJudgementFilter {
                status: Some("ok".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(ok.total, 2);
        assert_eq!(ok.records.len(), 2);

        // 分页：limit=2 offset=2 → records=2 且 total 仍为 7（计数不受分页影响）。
        let page = store
            .list_with_total(&HookJudgementFilter {
                limit: Some(2),
                offset: Some(2),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.total, 7);
        assert_eq!(page.records.len(), 2);

        // 过滤 + 分页组合：hook_type 过滤 total=7，limit=3 只取 3 条。
        let filtered_page = store
            .list_with_total(&HookJudgementFilter {
                hook_type: Some("assistant_match_topic".into()),
                limit: Some(3),
                offset: None,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(filtered_page.total, 7);
        assert_eq!(filtered_page.records.len(), 3);

        // offset 越界 → 空记录但 total 正确。
        let past_end = store
            .list_with_total(&HookJudgementFilter {
                offset: Some(100),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(past_end.total, 7);
        assert!(past_end.records.is_empty());
    }
}
