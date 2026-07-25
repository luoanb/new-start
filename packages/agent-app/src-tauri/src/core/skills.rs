use super::{
    conversation_store::now_ms,
    error::{AppError, AppResult},
    models::SkillInfo,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct SkillRegistry {
    skills: BTreeMap<String, SkillInfo>,
}

impl SkillRegistry {
    pub fn with_defaults() -> Self {
        let mut registry = Self {
            skills: BTreeMap::new(),
        };

        registry.register("get_current_time", "获取当前时间");
        registry.register(
            "calculate",
            "执行基础数学计算（占位规格，后续实现安全表达式求值）",
        );
        registry.register("echo", "回显消息");
        registry
    }

    pub fn list(&self) -> Vec<SkillInfo> {
        self.skills.values().cloned().collect()
    }

    pub fn execute_echo(&self, message: &str) -> AppResult<String> {
        self.ensure_exists("echo")?;
        Ok(message.to_string())
    }

    pub fn execute_time(&self) -> AppResult<String> {
        self.ensure_exists("get_current_time")?;
        Ok(now_ms().to_string())
    }

    fn register(&mut self, name: &str, description: &str) {
        self.skills.insert(
            name.to_string(),
            SkillInfo {
                name: name.to_string(),
                description: description.to_string(),
            },
        );
    }

    fn ensure_exists(&self, name: &str) -> AppResult<()> {
        if self.skills.contains_key(name) {
            Ok(())
        } else {
            Err(AppError::SkillNotFound(name.to_string()))
        }
    }
}
