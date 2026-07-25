use super::{
    error::{AppError, AppResult},
    models::{Conversation, Message},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
    sessions_dir: PathBuf,
}

impl Storage {
    pub fn default() -> AppResult<Self> {
        let root = std::env::current_dir()?.join(".agent-app");
        Self::new(root)
    }

    pub fn new(root: impl Into<PathBuf>) -> AppResult<Self> {
        let root = root.into();
        let sessions_dir = root.join("sessions");
        fs::create_dir_all(&sessions_dir)?;

        Ok(Self { root, sessions_dir })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create_conversation(&self, conversation_id: Option<String>) -> AppResult<Conversation> {
        let now = now_ms();
        let id = match conversation_id {
            Some(id) if !id.trim().is_empty() => id,
            Some(_) => {
                return Err(AppError::InvalidInput(
                    "Conversation id cannot be empty".into(),
                ))
            }
            None => format!("conv_{now}"),
        };

        let conversation = Conversation {
            id,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
        };

        self.save_conversation(&conversation)?;
        Ok(conversation)
    }

    pub fn get_conversation(&self, conversation_id: &str) -> AppResult<Option<Conversation>> {
        if conversation_id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Conversation id cannot be empty".into(),
            ));
        }

        let path = self.conversation_path(conversation_id);
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path)?;
        Ok(Some(serde_json::from_str(&content)?))
    }

    pub fn require_conversation(&self, conversation_id: &str) -> AppResult<Conversation> {
        self.get_conversation(conversation_id)?
            .ok_or_else(|| AppError::ConversationNotFound(conversation_id.to_string()))
    }

    pub fn list_conversations(&self) -> AppResult<Vec<Conversation>> {
        let mut conversations = Vec::new();

        for entry in fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }

            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };

            if let Ok(conversation) = serde_json::from_str::<Conversation>(&content) {
                conversations.push(conversation);
            }
        }

        conversations.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(conversations)
    }

    pub fn add_message(&self, conversation_id: &str, message: Message) -> AppResult<Conversation> {
        let mut conversation = match self.get_conversation(conversation_id)? {
            Some(conversation) => conversation,
            None => self.create_conversation(Some(conversation_id.to_string()))?,
        };

        conversation.messages.push(message);
        conversation.updated_at = now_ms();
        self.save_conversation(&conversation)?;
        Ok(conversation)
    }

    pub fn clear_conversation(&self, conversation_id: &str) -> AppResult<()> {
        if conversation_id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Conversation id cannot be empty".into(),
            ));
        }

        let path = self.conversation_path(conversation_id);
        if !path.exists() {
            return Err(AppError::ConversationNotFound(conversation_id.to_string()));
        }

        fs::remove_file(path)?;
        Ok(())
    }

    fn save_conversation(&self, conversation: &Conversation) -> AppResult<()> {
        let path = self.conversation_path(&conversation.id);
        let content = serde_json::to_string_pretty(conversation)?;
        fs::write(path, content)?;
        Ok(())
    }

    fn conversation_path(&self, conversation_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{conversation_id}.json"))
    }
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis()
}
