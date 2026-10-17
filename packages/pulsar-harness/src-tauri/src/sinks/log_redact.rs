//! Helpers for safe diagnostic log previews (truncate + redact secrets).

const DEFAULT_PREVIEW_CHARS: usize = 240;

/// Redact common secret patterns, then truncate for log fields.
pub fn preview_for_log(text: &str, max_chars: usize) -> String {
    let redacted = redact_secrets(text);
    truncate_chars(&redacted, max_chars)
}

pub fn preview_json_for_log(value: &serde_json::Value, max_chars: usize) -> String {
    preview_for_log(&value.to_string(), max_chars)
}

pub fn preview_default(text: &str) -> String {
    preview_for_log(text, DEFAULT_PREVIEW_CHARS)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    format!("{head}…({total} chars)")
}

fn redact_secrets(text: &str) -> String {
    let mut out = text.to_string();
    for field in [
        "api_key",
        "apiKey",
        "authorization",
        "password",
        "secret",
        "token",
    ] {
        out = replace_json_string_field(&out, field);
    }
    out = redact_bearer(&out);
    out = redact_sk_keys(&out);
    out
}

fn replace_json_string_field(text: &str, field: &str) -> String {
    let patterns = [format!("\"{field}\":\""), format!("\"{field}\": \"")];
    let mut out = text.to_string();
    for pattern in patterns {
        let mut search_from = 0;
        while let Some(rel) = out[search_from..].find(&pattern) {
            let start = search_from + rel + pattern.len();
            if let Some(end_rel) = out[start..].find('"') {
                let end = start + end_rel;
                out.replace_range(start..end, "[REDACTED]");
                search_from = start + "[REDACTED]".len() + 1;
            } else {
                break;
            }
        }
    }
    out
}

fn redact_bearer(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::new();
    let mut last = 0;
    let mut search = 0;
    while let Some(rel) = lower[search..].find("bearer ") {
        let start = search + rel;
        out.push_str(&text[last..start]);
        out.push_str("bearer ");
        let token_start = start + "bearer ".len();
        let token_end = text[token_start..]
            .find(|c: char| c.is_whitespace() || c == '"' || c == ',' || c == '}')
            .map(|n| token_start + n)
            .unwrap_or(text.len());
        out.push_str("[REDACTED_TOKEN]");
        last = token_end;
        search = token_end;
    }
    out.push_str(&text[last..]);
    out
}

fn redact_sk_keys(text: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    let mut search = 0;
    while let Some(rel) = text[search..].find("sk-") {
        let start = search + rel;
        out.push_str(&text[last..start]);
        let end = text[start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .map(|n| start + n)
            .unwrap_or(text.len());
        out.push_str("[REDACTED_API_KEY]");
        last = end;
        search = end;
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_api_key_json_and_sk() {
        let raw = r#"{"api_key":"secret-value","note":"ok"} sk-abc123XYZ rest"#;
        let preview = preview_for_log(raw, 500);
        assert!(preview.contains("[REDACTED]"));
        assert!(preview.contains("[REDACTED_API_KEY]"));
        assert!(!preview.contains("secret-value"));
        assert!(!preview.contains("sk-abc123XYZ"));
    }

    #[test]
    fn truncates_long_text() {
        let raw = "a".repeat(100);
        let preview = preview_for_log(&raw, 10);
        assert!(preview.contains("…(100 chars)"));
        assert!(preview.starts_with("aaaaaaaaaa"));
    }
}
