//! 代码块数据模型（语义搜索的检索单元与对外契约）。

use serde::{Deserialize, Serialize};

/// 块类型：决定检索加权的语义单元类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockType {
    /// 函数声明（function_item / function_declaration / function_definition / method_definition / method_declaration）
    Function,
    /// 结构体声明（struct_item / struct_specifier / Go type_declaration）
    Struct,
    /// 类声明（class_declaration / class_definition / class_specifier）
    Class,
    /// Rust impl 块
    Impl,
    /// Rust trait 声明
    Trait,
    /// 枚举声明（enum_item / enum_specifier）
    Enum,
    /// 接口声明（interface_declaration）
    Interface,
    /// 文件兜底块（无任何声明节点时整文件为一块）
    File,
    /// 启发式分块或未知语言（按空行切分的段落）
    Unknown,
}

impl BlockType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Struct => "struct",
            Self::Class => "class",
            Self::Impl => "impl",
            Self::Trait => "trait",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::File => "file",
            Self::Unknown => "unknown",
        }
    }

    /// 检索加权（叠加在 FTS5 bm25 之上）：容器/语义块加成，普通块无加成。
    pub fn weight(&self) -> f64 {
        match self {
            Self::Impl | Self::Trait | Self::Interface => 0.6,
            Self::Function | Self::Struct | Self::Class | Self::Enum => 0.3,
            Self::File | Self::Unknown => 0.0,
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "function" => Self::Function,
            "struct" => Self::Struct,
            "class" => Self::Class,
            "impl" => Self::Impl,
            "trait" => Self::Trait,
            "enum" => Self::Enum,
            "interface" => Self::Interface,
            "file" => Self::File,
            _ => Self::Unknown,
        }
    }
}

/// 索引中的原始块（写入 SQLite 的数据模型）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    /// 相对 workspace 根路径（`/` 分隔）。
    pub path: String,
    /// 1-based 起始行（含）。
    pub start_line: usize,
    /// 1-based 结束行（含）。
    pub end_line: usize,
    pub block_type: BlockType,
    /// 块正文（可能截断，供检索与展示）。
    pub content: String,
}

/// 对外检索结果条目（AI 工具 / Tauri command / 前端共用契约）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchBlock {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    /// 块类型字符串（`BlockType::as_str`）。
    pub block_type: String,
    /// 相关度分数（bm25 + 块类型加权）。
    pub score: f64,
    /// 内容摘要（默认 ≤ 400 字符）。
    pub content: String,
}

/// 语义搜索返回结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    pub results: Vec<SearchBlock>,
    /// 本次搜索所用索引的总块数（供前端/模型判断索引规模）。
    pub indexed_blocks: usize,
    /// 本次索引构建/增量耗时 ms（0 = 索引已就绪未重建）。
    pub index_duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_type_roundtrip() {
        for bt in [
            BlockType::Function,
            BlockType::Struct,
            BlockType::Class,
            BlockType::Impl,
            BlockType::Trait,
            BlockType::Enum,
            BlockType::Interface,
            BlockType::File,
            BlockType::Unknown,
        ] {
            assert_eq!(BlockType::from_str(bt.as_str()), bt);
        }
    }

    #[test]
    fn weight_ordering() {
        assert!(BlockType::Impl.weight() > BlockType::Function.weight());
        assert!(BlockType::Function.weight() > BlockType::File.weight());
        assert_eq!(BlockType::Unknown.weight(), 0.0);
    }

    #[test]
    fn search_block_serializes() {
        let b = SearchBlock {
            path: "src/main.rs".into(),
            start_line: 1,
            end_line: 3,
            block_type: "function".into(),
            score: 1.5,
            content: "fn main() {}".into(),
        };
        let v = serde_json::to_value(b).unwrap();
        assert_eq!(v["path"], "src/main.rs");
        assert_eq!(v["block_type"], "function");
    }
}
