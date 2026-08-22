//! 语法感知分块：tree-sitter 按顶层声明切块，未知语言回退启发式分块。
//!
//! 块边界 = 语义单元（函数/结构体/类/impl/trait/枚举/接口），而非行数硬切。
//! 这是语义搜索相对行级 `grep` 的核心差异：返回的是「完整代码块」。

use tree_sitter::{Language, Node, Parser};

use super::chunk::{BlockType, CodeChunk};

/// 单文件内容大小上限（超出跳过索引，防止超大文件拖垮构建）。
const MAX_FILE_BYTES: usize = 512 * 1024;
/// 单块正文上限（防止超大容器块/长文件污染检索与上下文）。
const MAX_BLOCK_CHARS: usize = 4000;

/// 语法感知分块器（无状态，可复用）。
pub struct Chunker;

impl Chunker {
    /// 按内容切块。`rel` 为相对 workspace 根的路径（用于语言识别与结果 path）。
    pub fn chunk(rel: &str, content: &str) -> Vec<CodeChunk> {
        if content.len() > MAX_FILE_BYTES {
            return Vec::new();
        }
        let Some(lang) = language_for_path(rel) else {
            return heuristic_chunks(rel, content);
        };
        let mut parser = Parser::new();
        if parser.set_language(&lang).is_err() {
            return heuristic_chunks(rel, content);
        }
        let Some(tree) = parser.parse(content, None) else {
            return heuristic_chunks(rel, content);
        };
        let mut chunks = Vec::new();
        collect_declarations(&tree.root_node(), rel, content, &mut chunks);
        if chunks.is_empty() {
            // 无任何可识别声明 → 整文件为一块（文件级兜底）。
            chunks.push(file_block(rel, content));
        }
        chunks
    }
}

/// 递归收集声明块：命中声明节点即切块并停止下钻（避免嵌套重复）；
/// 非声明节点继续下钻，以捕获 `export function` / `impl` 内等包裹场景。
fn collect_declarations(node: &Node, rel: &str, source: &str, out: &mut Vec<CodeChunk>) {
    if let Some(bt) = block_type_for_kind(node.kind()) {
        out.push(chunk_from_node(rel, source, node, bt));
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_declarations(&child, rel, source, out);
    }
}

fn chunk_from_node(rel: &str, source: &str, node: &Node, bt: BlockType) -> CodeChunk {
    let start_line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;
    CodeChunk {
        path: rel.to_string(),
        start_line,
        end_line,
        block_type: bt,
        content: extract_lines(source, start_line, end_line),
    }
}

/// 按 1-based 行区间取正文（含两端），并截断到单块上限。
fn extract_lines(source: &str, start_line: usize, end_line: usize) -> String {
    let mut out = String::new();
    for (idx, line) in source.lines().enumerate() {
        let lineno = idx + 1;
        if lineno >= start_line && lineno <= end_line {
            out.push_str(line);
            out.push('\n');
        }
        if lineno >= end_line {
            break;
        }
    }
    truncate(&out, MAX_BLOCK_CHARS)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push_str("…");
        out
    }
}

fn file_block(rel: &str, content: &str) -> CodeChunk {
    let line_count = content.lines().count().max(1);
    CodeChunk {
        path: rel.to_string(),
        start_line: 1,
        end_line: line_count,
        block_type: BlockType::File,
        content: truncate(content, MAX_BLOCK_CHARS),
    }
}

/// 未知语言回退：按空行切分成段落块。对脚本/标记类文件足够；
/// 结构缺失（`block_type = unknown`）但检索仍可用。
fn heuristic_chunks(rel: &str, content: &str) -> Vec<CodeChunk> {
    let lines: Vec<&str> = content.lines().collect();
    let mut chunks = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        let start_line = i + 1;
        let mut j = i;
        while j < lines.len() && !lines[j].trim().is_empty() {
            j += 1;
        }
        let end_line = j; // lines[i..j] 是段落，j 可能 == lines.len()（无尾空行）
        chunks.push(CodeChunk {
            path: rel.to_string(),
            start_line,
            end_line,
            block_type: BlockType::Unknown,
            content: truncate(&lines[i..j].join("\n"), MAX_BLOCK_CHARS),
        });
        i = j + 1;
    }
    if chunks.is_empty() {
        chunks.push(file_block(rel, content));
    }
    chunks
}

/// 语言识别：扩展名 → tree-sitter grammar。未知 → None（回退启发式）。
fn language_for_path(rel: &str) -> Option<Language> {
    let ext = rel.rsplit('.').next()?;
    let lang: Language = match ext {
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        "ts" | "mts" | "cts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "js" | "mjs" | "cjs" | "jsx" => tree_sitter_javascript::LANGUAGE.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        "py" => tree_sitter_python::LANGUAGE.into(),
        "java" => tree_sitter_java::LANGUAGE.into(),
        "c" => tree_sitter_c::LANGUAGE.into(),
        "cpp" | "cc" | "cxx" | "h" | "hh" | "hpp" => tree_sitter_cpp::LANGUAGE.into(),
        _ => return None,
    };
    Some(lang)
}

/// 节点 kind → 块类型。未知声明 kinds 归 None（继续下钻或忽略）。
fn block_type_for_kind(kind: &str) -> Option<BlockType> {
    match kind {
        "function_item"           // Rust
        | "function_declaration"  // TS/JS
        | "function_definition"   // Python/Go/C/C++
        | "method_definition"     // TS/JS
        | "method_declaration"    // Go/Java
        => Some(BlockType::Function),
        "struct_item"             // Rust
        | "struct_specifier"      // C/C++
        | "type_declaration"      // Go（type X struct/interface/...）
        => Some(BlockType::Struct),
        "enum_item"               // Rust
        | "enum_specifier"        // C/C++
        => Some(BlockType::Enum),
        "impl_item" => Some(BlockType::Impl),
        "trait_item" => Some(BlockType::Trait),
        "class_declaration"       // TS/JS
        | "class_definition"      // Python
        | "class_specifier"       // C++
        => Some(BlockType::Class),
        "interface_declaration"   // TS
        => Some(BlockType::Interface),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_function_chunking() {
        let src = "fn helper() {}\n\nfn main() {\n    helper();\n}\n";
        let chunks = Chunker::chunk("src/main.rs", src);
        assert_eq!(chunks.len(), 2, "两个函数应各成一块: {chunks:?}");
        let main = chunks.iter().find(|c| c.content.contains("fn main")).unwrap();
        assert_eq!(main.start_line, 3);
        assert_eq!(main.end_line, 5);
        assert_eq!(main.block_type, BlockType::Function);
    }

    #[test]
    fn rust_impl_is_container() {
        let src = "struct A;\nimpl A {\n    fn m(&self) {}\n}\n";
        let chunks = Chunker::chunk("src/lib.rs", src);
        // struct 一块 + impl 一块（impl 内 method 不重复切）。
        assert!(chunks.iter().any(|c| c.block_type == BlockType::Struct));
        assert!(chunks.iter().any(|c| c.block_type == BlockType::Impl));
        assert_eq!(chunks.len(), 2, "不应把 impl 内方法单独再切: {chunks:?}");
    }

    #[test]
    fn ts_exported_function() {
        let src = "export function add(a: number, b: number): number {\n  return a + b;\n}\n";
        let chunks = Chunker::chunk("src/util.ts", src);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].block_type, BlockType::Function);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 3);
    }

    #[test]
    fn unknown_language_falls_back_to_heuristic() {
        let src = "line1\nline2\n\nline3\n";
        let chunks = Chunker::chunk("notes.txt", src);
        assert_eq!(chunks.len(), 2, "按空行切两段: {chunks:?}");
        assert!(chunks.iter().all(|c| c.block_type == BlockType::Unknown));
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 2);
        assert_eq!(chunks[1].start_line, 4);
        assert_eq!(chunks[1].end_line, 4);
    }

    #[test]
    fn empty_declaration_falls_back_to_file_block() {
        let src = "no declarations here\n";
        // Rust 解析任意文本可能无声明节点；即使解析失败也回退启发式/文件块。
        let chunks = Chunker::chunk("src/main.rs", src);
        assert!(!chunks.is_empty(), "应至少有一个兜底块");
    }
}
