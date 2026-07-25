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
    StorageError(String),
    RuntimeError(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::InvalidInput(_) => "invalid_input",
            AppError::ConversationNotFound(_) => "conversation_not_found",
            AppError::SkillNotFound(_) => "skill_not_found",
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
            | AppError::SkillNotFound(_) => 2,
            AppError::StorageError(_) | AppError::RuntimeError(_) => 1,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::InvalidInput(message)
            | AppError::StorageError(message)
            | AppError::RuntimeError(message) => write!(f, "{message}"),
            AppError::ConversationNotFound(id) => write!(f, "Conversation not found: {id}"),
            AppError::SkillNotFound(name) => write!(f, "Skill not found: {name}"),
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
