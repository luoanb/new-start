use serde::Serialize;
use std::{error::Error, fmt, io};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Serialize)]
pub struct AppErrorPayload {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug)]
pub enum AppError {
    InvalidInput(String),
    ConversationNotFound(String),
    SkillNotFound(String),
    ProviderNotFound(String),
    ModelNotFound(String),
    ProviderAuthMissing(String),
    LlmRequestFailed(String),
    StorageError(String),
    RuntimeError(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::InvalidInput(_) => "invalid_input",
            AppError::ConversationNotFound(_) => "conversation_not_found",
            AppError::SkillNotFound(_) => "skill_not_found",
            AppError::ProviderNotFound(_) => "provider_not_found",
            AppError::ModelNotFound(_) => "model_not_found",
            AppError::ProviderAuthMissing(_) => "provider_auth_missing",
            AppError::LlmRequestFailed(_) => "llm_request_failed",
            AppError::StorageError(_) => "storage_error",
            AppError::RuntimeError(_) => "runtime_error",
        }
    }

    pub fn payload(&self) -> AppErrorPayload {
        AppErrorPayload {
            code: self.code(),
            message: self.to_string(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::InvalidInput(_)
            | AppError::ConversationNotFound(_)
            | AppError::SkillNotFound(_)
            | AppError::ProviderNotFound(_)
            | AppError::ModelNotFound(_)
            | AppError::ProviderAuthMissing(_) => 2,
            AppError::LlmRequestFailed(_)
            | AppError::StorageError(_)
            | AppError::RuntimeError(_) => 1,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::InvalidInput(message)
            | AppError::ProviderAuthMissing(message)
            | AppError::LlmRequestFailed(message)
            | AppError::StorageError(message)
            | AppError::RuntimeError(message) => write!(f, "{message}"),
            AppError::ConversationNotFound(id) => write!(f, "Conversation not found: {id}"),
            AppError::SkillNotFound(name) => write!(f, "Skill not found: {name}"),
            AppError::ProviderNotFound(id) => write!(f, "Provider not found: {id}"),
            AppError::ModelNotFound(id) => write!(f, "Model not found: {id}"),
        }
    }
}

impl Error for AppError {}

impl From<io::Error> for AppError {
    fn from(error: io::Error) -> Self {
        AppError::StorageError(error.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        AppError::StorageError(error.to_string())
    }
}
