//! Insert catalog: caller-facing manuals embedded from `inserts/<id>.md` (scheme 2).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rust_embed::Embed;

use super::error::{AppError, AppResult};

#[derive(Embed)]
#[folder = "inserts/"]
struct Inserts;

/// Process-wide catalog of self-describing atom manuals.
pub struct InsertCatalog;

fn cache() -> &'static Mutex<HashMap<String, &'static str>> {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

impl InsertCatalog {
    fn path_for(id: &str) -> String {
        format!("{id}.md")
    }

    /// Whether an insert file exists for `id`.
    pub fn exists(id: &str) -> bool {
        Inserts::get(&Self::path_for(id)).is_some()
    }

    /// Full manual text for `id`.
    pub fn get(id: &str) -> AppResult<&'static str> {
        if let Some(hit) = cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .copied()
        {
            return Ok(hit);
        }

        let path = Self::path_for(id);
        let file = Inserts::get(&path).ok_or_else(|| {
            AppError::RuntimeError(format!("missing insert: inserts/{path}"))
        })?;
        let text = std::str::from_utf8(file.data.as_ref()).map_err(|e| {
            AppError::RuntimeError(format!("insert {path} is not utf-8: {e}"))
        })?;
        let leaked: &'static str = Box::leak(text.to_string().into_boxed_str());
        cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), leaked);
        Ok(leaked)
    }

    /// Mount-point helper: missing insert fails fast.
    pub fn require(id: &str) -> &'static str {
        Self::get(id).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Feed model: system neuron content + tool insert (content first).
    pub fn system_with_insert(neuron_content: &str, insert_id: &str) -> String {
        let insert = Self::require(insert_id);
        format!("{neuron_content}\n\n{insert}")
    }

    /// Short projection: `## 工具` + `## 对模型的期待` (fallback: legacy 作用/用法, else full).
    pub fn summary(id: &str) -> AppResult<String> {
        let full = Self::get(id)?;
        let tool = section_body(full, "工具");
        let expect = section_body(full, "对模型的期待");
        if tool.is_some() || expect.is_some() {
            return Ok(join_sections(tool, expect));
        }
        let purpose = section_body(full, "作用");
        let usage = section_body(full, "用法");
        if purpose.is_some() || usage.is_some() {
            return Ok(join_sections(purpose, usage));
        }
        Ok(full.to_string())
    }
}

fn join_sections(a: Option<&str>, b: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(a) = a {
        out.push_str(a);
    }
    if let Some(b) = b {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(b);
    }
    out
}

fn section_body<'a>(markdown: &'a str, heading: &str) -> Option<&'a str> {
    let marker = format!("## {heading}");
    let start = markdown.find(&marker)? + marker.len();
    let rest = markdown[start..].trim_start();
    let end = rest.find("\n## ").unwrap_or(rest.len());
    let body = rest[..end].trim();
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_known_model_atoms() {
        for id in [
            "assistant.score_feedback",
            "assistant.match_topic",
            "assistant.complete_scope",
            "neuron.draft_from_model",
            "neuron.select_one",
        ] {
            assert!(InsertCatalog::exists(id), "expected insert for {id}");
            let text = InsertCatalog::require(id);
            assert!(text.contains("## 工具"), "{id} missing 工具");
            assert!(text.contains("## 对模型的期待"), "{id} missing 对模型的期待");
        }
        assert!(!InsertCatalog::exists("neuron.ensure_system"));
        assert!(!InsertCatalog::exists("neuron.bootstrap_system"));
    }

    #[test]
    fn get_missing_returns_error() {
        let err = InsertCatalog::get("does.not.exist").unwrap_err();
        assert!(err.to_string().contains("missing insert"));
    }

    #[test]
    fn system_with_insert_concatenates() {
        let out = InsertCatalog::system_with_insert("ROLE", "neuron.select_one");
        assert!(out.starts_with("ROLE\n\n"));
        assert!(out.contains("## 工具"));
    }

    #[test]
    fn summary_extracts_tool_and_expect() {
        let summary = InsertCatalog::summary("neuron.select_one").expect("summary");
        assert!(!summary.is_empty());
        assert!(!summary.contains("## 忌用"));
    }
}
