//! OpenAI 兼容 Chat Completions 协议封装层。
//!
//! 职责边界：
//! - 本模块只做「OpenAI Chat Completions 协议」的序列化 / 反序列化 / HTTP 发送 / SSE 流式解析。
//! - **不含**任何服务商策略、参数抹平、模型能力判断——那些属于 `providers`（整合层）。
//! - 服务商 / 模型治理字段（reasoning_effort、thinking 等特异性参数）通过 `extra` 透传，
//!   由 `providers` 按需填充，本层不感知。
//!
//! 依赖：`serde` + `serde_json` + `reqwest`（`json`、`rustls-tls`），**不依赖 async-openai**。

use std::{borrow::Cow, collections::BTreeMap};

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::error::{AppError, AppResult};

/// 消息内容：纯文本或多模态部分列表（`image_url`/`input_audio`/`text`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

/// 结构化输出契约（值类型，随 hook 走，不持有 hook 业务数据）。
///
/// wire 形态（经 `extra` 扁平透传为请求体顶层 `response_format`）：
/// - `JsonSchema` → `{"type":"json_schema","json_schema":{...}}`
/// - `JsonObject` → `{"type":"json_object"}`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFormatSpec {
    /// 显式 JSON Schema（schema 原文，服务商要求对象形态时反序列化后注入）。
    JsonSchema(Cow<'static, str>),
    /// 仅要求输出 JSON 对象（不校验结构）。
    JsonObject,
}

impl MessageContent {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }
}

/// 多模态内容块（OpenAI 契约：`type` + 各自载荷）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: Value },
    InputAudio { input_audio: Value },
}

/// 工具调用（assistant 消息内 / 响应内）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolCallWire {
    pub id: String,
    #[serde(default)]
    pub r#type: String,
    pub function: FunctionCallWire,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCallWire {
    pub name: String,
    /// 参数为 JSON 字符串（OpenAI 契约如此）。
    pub arguments: String,
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
    /// 推理模型 token 上限（与 max_tokens 二选一）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    // ── 流式 ──
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    // ── 其它 ──
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
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
            max_completion_tokens: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            seed: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            stream: None,
            n: None,
            logprobs: None,
            top_logprobs: None,
            user: None,
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
    #[serde(default)]
    pub logprobs: Option<Value>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
    #[serde(default)]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompletionTokensDetails {
    #[serde(default)]
    pub reasoning_tokens: Option<u32>,
}

// ── 流式响应（SSE chunk） ──────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StreamChunk {
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub choices: Vec<StreamChoice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StreamChoice {
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub delta: StreamDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct StreamDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallWire>>,
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
            phase = "llm_request_out",
            body = %serde_json::to_string(body).unwrap_or_default(),
            "llm request body (full)"
        );
    }
}

fn dump_wire_response(bytes: &[u8]) {
    if DUMP_LLM_WIRE {
        tracing::info!(
            phase = "llm_response_in",
            body = %String::from_utf8_lossy(bytes),
            "llm response body (full)"
        );
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
        serde_json::from_slice(&bytes)
            .map_err(|e| AppError::LlmRequestFailed(format!("parse response: {e}")))
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
                        logprobs: None,
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
                        for tc in tool_calls {
                            let idx = tc.r#type.is_empty() as usize; // fallback
                            let _ = idx;
                            match calls.iter_mut().find(|c| c.id == tc.id) {
                                Some(existing) if !tc.function.arguments.is_empty() => {
                                    existing.function.arguments.push_str(&tc.function.arguments);
                                }
                                Some(_) => {}
                                None => {
                                    calls.push(tc.clone());
                                }
                            }
                        }
                    }
                    if let Some(fr) = &choice.finish_reason {
                        acc.finish_reason = Some(fr.clone());
                    }
                }
            }
        }
        aggregated.choices = final_choice.into_iter().collect();
        if DUMP_LLM_WIRE {
            tracing::info!(
                phase = "llm_response_in",
                body = %serde_json::to_string(&aggregated).unwrap_or_default(),
                "llm stream response (aggregated full)"
            );
        }
        Ok(aggregated)
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parse_stream_chunk() {
        let json = r#"{"id":"c","object":"chat.completion.chunk","created":1,"model":"m",
            "choices":[{"index":0,"delta":{"reasoning_content":"think","content":"hi"},
            "finish_reason":null}]}"#;
        let parsed: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.choices[0].delta.reasoning_content.as_deref(), Some("think"));
        assert_eq!(parsed.choices[0].delta.content.as_deref(), Some("hi"));
    }
}
