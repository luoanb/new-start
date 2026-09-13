//! OpenAI 兼容 Chat Completions 协议封装层。
//!
//! 职责边界：
//! - 本模块只做「OpenAI Chat Completions 协议」的序列化 / 反序列化 / HTTP 发送 / SSE 流式解析。
//! - **不含**任何服务商策略、参数抹平、模型能力判断——那些属于 `providers`（整合层）。
//! - 服务商 / 模型治理字段（reasoning_effort、thinking 等特异性参数）通过 `extra` 透传，
//!   由 `providers` 按需填充，本层不感知。
//!
//! 依赖：`serde` + `serde_json` + `reqwest`（`json`、`rustls-tls`），**不依赖 async-openai**。

use std::collections::BTreeMap;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::{AppError, AppResult};
use crate::core::models::{
    FunctionCallWire, RemoteModelInfo, StreamChunk, StreamToolCallDelta, ToolCallWire, Usage,
};
use crate::core::log_phase::{
    PHASE_LLM_CALL_PERF, PHASE_LLM_REQUEST_OUT, PHASE_LLM_RESPONSE_IN,
};

/// 消息内容：纯文本。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
}

impl MessageContent {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }
}

/// Chat Completions 消息（覆盖 system/user/assistant/tool 全 role）。
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<MessageContent>,
    /// 推理模型的思维链（DeepSeek：有工具调用的多轮必须回传，否则 400）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallWire>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: Some(MessageContent::text(content)),
            ..Default::default()
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: Some(MessageContent::text(content)),
            ..Default::default()
        }
    }
    pub fn assistant(content: Option<String>, tool_calls: Option<Vec<ToolCallWire>>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.map(MessageContent::text),
            tool_calls,
            ..Default::default()
        }
    }
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            tool_call_id: Some(tool_call_id.into()),
            content: Some(MessageContent::text(content)),
            ..Default::default()
        }
    }
    /// 回传推理思维链（多轮工具调用场景）。
    pub fn with_reasoning(mut self, reasoning: Option<String>) -> Self {
        self.reasoning_content = reasoning;
        self
    }
}

/// 工具定义（function schema）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub type_: String,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionDef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    /// strict 工具（结构化输出）：强制模型输出符合 schema 的调用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Chat Completions 请求（标准 OpenAI 契约）。
///
/// 特异性 / 未来扩展字段（reasoning_effort、thinking、response_format 等）通过 `extra` 扁平透传，
/// 由 `providers` 按服务商填充——本层不感知其语义。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    // ── 采样 ──
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 旧版 token 上限（非推理模型）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    // ── 工具 ──
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDef>>,
    /// 特异性 / 未来扩展字段扁平透传（reasoning_effort、thinking、response_format…）。
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            top_p: None,
            max_tokens: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            seed: None,
            tools: None,
            extra: BTreeMap::new(),
        }
    }
}

// ── 非流式响应 ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatResponse {
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub system_fingerprint: Option<String>,
    #[serde(default)]
    pub choices: Vec<ResponseChoice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseChoice {
    #[serde(default)]
    pub index: usize,
    pub message: ResponseMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ResponseMessage {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: Option<String>,
    /// 思维链（DeepSeek 等推理模型，与 content 同级）。
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallWire>>,
    #[serde(default)]
    pub refusal: Option<String>,
}

/// 流式聚合结果（与 `ChatResponse` 结构一致，便于复用解析）。
pub type StreamResult = ChatResponse;

// ── 客户端 ────────────────────────────────────────────────

/// 诊断开关：打印发给模型的完整请求体与原始响应体。
/// 排查"工具声明必填但模型返回空 arguments"等入参/出参问题时开启。
const DUMP_LLM_WIRE: bool = true;

fn dump_wire_request(body: &serde_json::Value) {
    if DUMP_LLM_WIRE {
        tracing::info!(
            phase = PHASE_LLM_REQUEST_OUT,
            body = %serde_json::to_string(body).unwrap_or_default(),
            "llm request body (full)"
        );
    }
}

fn dump_wire_response(bytes: &[u8]) {
    if DUMP_LLM_WIRE {
        tracing::info!(
            phase = PHASE_LLM_RESPONSE_IN,
            body = %String::from_utf8_lossy(bytes),
            "llm response body (full)"
        );
    }
}

/// 调用耗时打点（llm_call_perf）：墙钟 + usage 定量拆分慢因——
/// completion_tokens 巨大 → 服务端生成/思考长；completion 小而耗时高 → prefill/排队。
/// 流式额外记录首包时间（TTFB ≈ prefill 完成点）。
fn log_call_perf(
    stream: bool,
    model: &str,
    started: std::time::Instant,
    first_chunk_at: Option<std::time::Instant>,
    usage: Option<&Usage>,
) {
    tracing::info!(
        phase = PHASE_LLM_CALL_PERF,
        stream,
        model = %model,
        elapsed_ms = started.elapsed().as_millis() as u64,
        ttfb_ms = first_chunk_at
            .map(|t| (t - started).as_millis() as u64)
            .unwrap_or(0),
        prompt_tokens = usage.map_or(0, |u| u.prompt_tokens),
        completion_tokens = usage.map_or(0, |u| u.completion_tokens),
        reasoning_tokens = usage
            .and_then(|u| u.completion_tokens_details.as_ref())
            .and_then(|d| d.reasoning_tokens)
            .unwrap_or(0),
        "model call perf"
    );
}

/// 流式工具调用分片归并（OpenAI 契约：仅首个分片带 `id` / `name`，后续分片只带
/// `function.arguments` 片段）。
///
/// 归位规则：先按 `id` 命中已有调用续片（兼容「每个分片都重复携带 id」的实现），否则占用
/// `index` 槽位；槽位已被其他具名调用占用时追加新条目。`arguments` 片段按到达顺序拼接。
fn merge_tool_call_deltas(calls: &mut Vec<ToolCallWire>, deltas: &[StreamToolCallDelta]) {
    for delta in deltas {
        let named_id = delta.id.as_deref().filter(|id| !id.is_empty());
        let slot = match named_id {
            Some(id) => match calls.iter().position(|c| c.id == id) {
                Some(pos) => pos,
                None => {
                    let occupied = calls.len() > delta.index && !calls[delta.index].id.is_empty();
                    if occupied {
                        calls.len()
                    } else {
                        delta.index
                    }
                }
            },
            None => delta.index,
        };
        // 补齐缺口：index 跳号时中间槽位先占空条目，保证下标即 index。
        while calls.len() <= slot {
            calls.push(ToolCallWire {
                id: String::new(),
                r#type: "function".into(),
                function: FunctionCallWire {
                    name: String::new(),
                    arguments: String::new(),
                },
            });
        }
        let entry = &mut calls[slot];
        if let Some(id) = named_id {
            entry.id = id.to_string();
        }
        if let Some(kind) = delta.r#type.as_deref().filter(|t| !t.is_empty()) {
            entry.r#type = kind.to_string();
        }
        if let Some(name) = delta.function.name.as_deref().filter(|n| !n.is_empty()) {
            entry.function.name = name.to_string();
        }
        if let Some(fragment) = &delta.function.arguments {
            entry.function.arguments.push_str(fragment);
        }
    }
}

/// 轻量 OpenAI 兼容客户端：仅负责 HTTP 发送与错误归一。
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl Client {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        }
    }

    /// 非流式调用：`POST {base}/chat/completions`，返回完整响应。
    pub async fn chat(&self, req: &ChatRequest) -> AppResult<ChatResponse> {
        let body = serde_json::to_value(req)
            .map_err(|e| AppError::LlmRequestFailed(format!("serialize request: {e}")))?;
        dump_wire_request(&body);
        let started = std::time::Instant::now();
        let response = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::LlmRequestFailed(format!("request failed: {e}")))?;
        let bytes = self.read_body(response).await?;
        dump_wire_response(&bytes);
        let parsed: ChatResponse = serde_json::from_slice(&bytes)
            .map_err(|e| AppError::LlmRequestFailed(format!("parse response: {e}")))?;
        log_call_perf(false, &parsed.model, started, None, parsed.usage.as_ref());
        Ok(parsed)
    }

    /// 流式调用：SSE 逐 chunk 解析，通过 `on_chunk` 回调抛出；结束后返回聚合结果。
    pub async fn chat_stream<F>(&self, req: &ChatRequest, mut on_chunk: F) -> AppResult<StreamResult>
    where
        F: FnMut(StreamChunk),
    {
        let mut body = serde_json::to_value(req)
            .map_err(|e| AppError::LlmRequestFailed(format!("serialize request: {e}")))?;
        // 强制流式
        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".into(), serde_json::json!(true));
        }
        dump_wire_request(&body);
        let response = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::LlmRequestFailed(format!("request failed: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            let bytes = response
                .bytes()
                .await
                .map_err(|e| AppError::LlmRequestFailed(format!("read error body: {e}")))?;
            return Err(self.map_error(status, &bytes));
        }

        let mut stream = response.bytes_stream();
        let started = std::time::Instant::now();
        let mut first_chunk_at: Option<std::time::Instant> = None;
        let mut aggregated = StreamResult {
            id: String::new(),
            object: String::new(),
            created: 0,
            model: String::new(),
            system_fingerprint: None,
            choices: vec![],
            usage: None,
        };
        // 累积最终的 choices（含 finish_reason 与 usage）。
        let mut final_choice: Option<ResponseChoice> = None;
        use futures_util::StreamExt;

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res
                .map_err(|e| AppError::LlmRequestFailed(format!("stream error: {e}")))?;
            let text = String::from_utf8_lossy(&chunk);
            if first_chunk_at.is_none() {
                first_chunk_at = Some(std::time::Instant::now());
            }
            for line in text.lines() {
                let line = line.trim();
                if !line.starts_with("data:") {
                    continue;
                }
                let data = line[5..].trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let parsed: StreamChunk = match serde_json::from_str(data) {
                    Ok(c) => c,
                    Err(_) => continue, // 跳过无法解析的 keep-alive / 注释行
                };
                if aggregated.id.is_empty() {
                    aggregated.id = parsed.id.clone();
                    aggregated.model = parsed.model.clone();
                    aggregated.created = parsed.created;
                    aggregated.object = parsed.object.clone();
                }
                on_chunk(parsed.clone());
                if let Some(usage) = parsed.usage {
                    aggregated.usage = Some(usage);
                }
                if let Some(choice) = parsed.choices.first() {
                    let acc = final_choice.get_or_insert_with(|| ResponseChoice {
                        index: choice.index,
                        message: ResponseMessage::default(),
                        finish_reason: None,
                    });
                    // 拼接 delta 内容
                    if let Some(content) = &choice.delta.content {
                        acc.message
                            .content
                            .get_or_insert_with(String::new)
                            .push_str(content);
                    }
                    if let Some(reasoning) = &choice.delta.reasoning_content {
                        acc.message
                            .reasoning_content
                            .get_or_insert_with(String::new)
                            .push_str(reasoning);
                    }
                    if let Some(tool_calls) = &choice.delta.tool_calls {
                        let calls = acc
                            .message
                            .tool_calls
                            .get_or_insert_with(Vec::new);
                        // OpenAI 流式工具调用按 index 分片，逐片拼接 arguments。
                        merge_tool_call_deltas(calls, tool_calls);
                    }
                    if let Some(fr) = &choice.finish_reason {
                        acc.finish_reason = Some(fr.clone());
                    }
                }
            }
        }
        aggregated.choices = final_choice.into_iter().collect();
        log_call_perf(
            true,
            &aggregated.model,
            started,
            first_chunk_at,
            aggregated.usage.as_ref(),
        );
        if DUMP_LLM_WIRE {
            tracing::info!(
                phase = PHASE_LLM_RESPONSE_IN,
                body = %serde_json::to_string(&aggregated).unwrap_or_default(),
                "llm stream response (aggregated full)"
            );
        }
        Ok(aggregated)
    }

    /// List models：`GET {base}/models`（OpenAI List models 契约）。
    ///
    /// 只做协议解析；「服务商是否实现该端点、字段是否扩展」由调用方（providers）消化。
    pub async fn list_models(&self) -> AppResult<Vec<RemoteModelInfo>> {
        let response = self
            .http
            .get(self.models_endpoint())
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| AppError::LlmRequestFailed(format!("request failed: {e}")))?;
        let bytes = self.read_body(response).await?;
        dump_wire_response(&bytes);
        parse_models_response(&bytes)
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    fn models_endpoint(&self) -> String {
        format!("{}/models", self.base_url)
    }

    async fn read_body(&self, response: reqwest::Response) -> AppResult<Vec<u8>> {
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AppError::LlmRequestFailed(format!("read response: {e}")))?;
        if !status.is_success() {
            return Err(self.map_error(status, &bytes));
        }
        Ok(bytes.to_vec())
    }

    /// 归一化非 2xx 错误：尝试解析 OpenAI 结构化错误 `error{message,type,code}`，否则原样输出。
    fn map_error(&self, status: StatusCode, bytes: &[u8]) -> AppError {
        #[derive(Deserialize)]
        struct ApiErrorEnvelope {
            #[serde(default)]
            error: Option<ApiErrorBody>,
        }
        #[derive(Deserialize)]
        struct ApiErrorBody {
            #[serde(default)]
            message: Option<String>,
            #[serde(default)]
            code: Option<String>,
        }
        let detail = String::from_utf8_lossy(bytes).to_string();
        let message = serde_json::from_slice::<ApiErrorEnvelope>(bytes)
            .ok()
            .and_then(|e| e.error)
            .and_then(|e| e.message)
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| {
                let code = serde_json::from_slice::<ApiErrorEnvelope>(bytes)
                    .ok()
                    .and_then(|e| e.error)
                    .and_then(|e| e.code)
                    .unwrap_or_default();
                if code.is_empty() {
                    detail
                } else {
                    format!("{code}: {detail}")
                }
            });
        AppError::LlmRequestFailed(format!("provider returned {status}: {message}"))
    }
}

/// 解析 `GET /models` 响应：官方契约为 `{ "object": "list", "data": [...] }`。
fn parse_models_response(bytes: &[u8]) -> AppResult<Vec<RemoteModelInfo>> {
    #[derive(Deserialize)]
    struct ModelsEnvelope {
        #[serde(default)]
        data: Vec<RemoteModelInfo>,
    }
    let parsed: ModelsEnvelope = serde_json::from_slice(bytes)
        .map_err(|e| AppError::LlmRequestFailed(format!("parse models response: {e}")))?;
    Ok(parsed.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::FunctionCallWire;

    #[test]
    fn message_serialization_roundtrip() {
        let msg = ChatMessage::assistant(
            Some("hi".into()),
            Some(vec![ToolCallWire {
                id: "call_1".into(),
                r#type: "function".into(),
                function: FunctionCallWire {
                    name: "weather".into(),
                    arguments: r#"{"city":"beijing"}"#.into(),
                },
            }]),
        );
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["role"], "assistant");
        assert_eq!(v["content"], "hi");
        assert_eq!(v["tool_calls"][0]["function"]["arguments"], r#"{"city":"beijing"}"#);
    }

    #[test]
    fn request_extra_flatten() {
        let mut req = ChatRequest::new("deepseek-chat", vec![ChatMessage::user("hello")]);
        req.extra.insert("thinking".into(), serde_json::json!({"type": "enabled"}));
        req.extra.insert("reasoning_effort".into(), serde_json::json!("high"));
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["thinking"]["type"], "enabled");
        assert_eq!(v["reasoning_effort"], "high");
        assert_eq!(v["model"], "deepseek-chat");
    }

    #[test]
    fn parse_non_stream_response() {
        let json = r#"{
            "id":"chatcmpl-x","object":"chat.completion","created":1700000000,"model":"gpt-4",
            "choices":[{"index":0,"finish_reason":"tool_calls","message":{
                "role":"assistant","content":null,
                "tool_calls":[{"id":"c1","type":"function","function":{"name":"f","arguments":"{}"}}]
            }}],
            "usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,
                "completion_tokens_details":{"reasoning_tokens":3}}
        }"#;
        let parsed: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.choices[0].finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(
            parsed.choices[0].message.tool_calls.as_ref().unwrap()[0].function.name,
            "f"
        );
        assert_eq!(
            parsed.usage.unwrap().completion_tokens_details.unwrap().reasoning_tokens,
            Some(3)
        );
    }

    #[test]
    fn parse_models_response_extracts_ids() {
        // 混合实测形态：标准 4 字段 + 非标准 `display_name` / `type`（type 被忽略）。
        let json = r#"{
            "object":"list",
            "data":[
                {"id":"gpt-5.6-sol","object":"model","created":1780876800,"owned_by":"openai","type":"model","display_name":"GPT-5.6 Sol"},
                {"id":"gpt-4o-mini","object":"model"}
            ]
        }"#;
        let models = parse_models_response(json.as_bytes()).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gpt-5.6-sol");
        assert_eq!(models[0].owned_by.as_deref(), Some("openai"));
        assert_eq!(models[0].display_name.as_deref(), Some("GPT-5.6 Sol"));
        assert_eq!(models[1].id, "gpt-4o-mini");
        assert_eq!(models[1].created, None);
        // 无 display_name 扩展时保持 None，由前端回落 id。
        assert_eq!(models[1].display_name, None);
    }

    #[test]
    fn parse_models_response_rejects_malformed_body() {
        assert!(parse_models_response(b"not json").is_err());
    }

    #[test]
    fn parse_stream_chunk() {
        let json = r#"{"id":"c","object":"chat.completion.chunk","created":1,"model":"m",
            "choices":[{"index":0,"delta":{"reasoning_content":"think","content":"hi"},
            "finish_reason":null}]}"#;
        let parsed: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.choices[0].delta.reasoning_content.as_deref(), Some("think"));
        assert_eq!(parsed.choices[0].delta.content.as_deref(), Some("hi"));
    }

    /// 后续分片只带 index + arguments 片段（无 id / name），必须能解析而不是被整块丢弃。
    #[test]
    fn parse_stream_tool_call_fragment_without_id() {
        let json = r#"{"id":"c","object":"chat.completion.chunk","created":1,"model":"m",
            "choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"query\":"}}]},
            "finish_reason":null}]}"#;
        let parsed: StreamChunk = serde_json::from_str(json).unwrap();
        let deltas = parsed.choices[0].delta.tool_calls.as_ref().unwrap();
        assert_eq!(deltas[0].index, 0);
        assert_eq!(deltas[0].id, None);
        assert_eq!(deltas[0].function.name, None);
        assert_eq!(deltas[0].function.arguments.as_deref(), Some(r#"{"query":"#));
    }

    /// 首片（带 id/name）+ 后续参数片段 → 按 index 拼接出完整 arguments；
    /// 并行调用（index 0/1）各自归位不串片。
    #[test]
    fn merge_tool_call_deltas_by_index() {
        let deltas = |json: &str| -> Vec<StreamToolCallDelta> { serde_json::from_str(json).unwrap() };
        let mut calls: Vec<ToolCallWire> = Vec::new();

        merge_tool_call_deltas(
            &mut calls,
            &deltas(r#"[{"index":0,"id":"call_a","type":"function","function":{"name":"search","arguments":""}}]"#),
        );
        merge_tool_call_deltas(&mut calls, &deltas(r#"[{"index":0,"function":{"arguments":"{\"query\":"}}]"#));
        merge_tool_call_deltas(&mut calls, &deltas(r#"[{"index":0,"function":{"arguments":"\"rust\"}"}}]"#));
        merge_tool_call_deltas(
            &mut calls,
            &deltas(r#"[{"index":1,"id":"call_b","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"a\"}"}}]"#),
        );

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_a");
        assert_eq!(calls[0].function.name, "search");
        assert_eq!(calls[0].function.arguments, r#"{"query":"rust"}"#);
        assert_eq!(calls[1].id, "call_b");
        assert_eq!(calls[1].function.name, "read_file");
        assert_eq!(calls[1].function.arguments, r#"{"path":"a"}"#);
        // 拼接结果必须是可解析的 JSON（原缺陷即此处退化为 Null）。
        assert!(serde_json::from_str::<serde_json::Value>(&calls[0].function.arguments).is_ok());
    }

    /// 兼容「每个分片都重复携带 id」的非标准实现：按 id 续片而非重复建条目。
    #[test]
    fn merge_tool_call_deltas_repeated_id() {
        let deltas = |json: &str| -> Vec<StreamToolCallDelta> { serde_json::from_str(json).unwrap() };
        let mut calls: Vec<ToolCallWire> = Vec::new();

        merge_tool_call_deltas(
            &mut calls,
            &deltas(r#"[{"index":0,"id":"call_a","function":{"name":"search","arguments":"{\"q\":"}}]"#),
        );
        merge_tool_call_deltas(&mut calls, &deltas(r#"[{"index":0,"id":"call_a","function":{"arguments":"1}"}}]"#));
        merge_tool_call_deltas(
            &mut calls,
            &deltas(r#"[{"index":0,"id":"call_b","function":{"name":"search","arguments":"{\"q\":2}"}}]"#),
        );

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.arguments, r#"{"q":1}"#);
        assert_eq!(calls[1].id, "call_b");
        assert_eq!(calls[1].function.arguments, r#"{"q":2}"#);
    }
}
