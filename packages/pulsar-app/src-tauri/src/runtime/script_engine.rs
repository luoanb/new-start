//! mlua 脚本引擎封装：最小执行能力（编译并执行一段 Lua 源码）。
//!
//! - 每个 `ScriptEngine` 实例持有一个独立的 Lua VM，避免脚本间状态互相污染。
//! - `mlua::Lua` 非 `Send`，实例不做共享；并发模型在宿主 API 步定义。
//! - Lua 结果值（number/string/boolean/nil/array/table）映射为 JSON；
//!   不可转换类型返回可读错误。

use mlua::{Lua, Value};

use crate::core::error::{AppError, AppResult};

/// 最小脚本执行引擎。
pub struct ScriptEngine {
    lua: Lua,
}

impl ScriptEngine {
    /// 创建独立 Lua VM（仅加载 safe 标准库子集）。
    pub fn new() -> AppResult<Self> {
        Ok(Self { lua: Lua::new() })
    }

    /// 执行一段 Lua 源码，返回其最后一个表达式的 JSON 值。
    pub fn eval(&self, source: &str) -> AppResult<serde_json::Value> {
        let value = self
            .lua
            .load(source)
            .eval::<Value>()
            .map_err(|e| AppError::RuntimeError(format!("script eval failed: {e}")))?;
        value_to_json(&value).map_err(|e| {
            AppError::RuntimeError(format!("script result not JSON-convertible: {e}"))
        })
    }
}

/// Lua 值 → JSON 值。number/string/boolean/nil/array/table 可转换，
/// function/thread/userdata/light-userdata/error 返回可读错误。
fn value_to_json(value: &Value) -> Result<serde_json::Value, String> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Integer(i) => Ok(serde_json::Value::from(*i)),
        Value::Number(n) => Ok(serde_json::Value::from(*n)),
        Value::String(s) => Ok(serde_json::Value::String(
            s.to_str().map_err(|e| e.to_string())?.to_string(),
        )),
        Value::Table(t) => table_to_json(t),
        other => Err(format!("unsupported Lua value type: {other:?}")),
    }
}

/// table → JSON：纯数组语义（1..n 连续整数键）转 Array，否则转 Object。
fn table_to_json(table: &mlua::Table) -> Result<serde_json::Value, String> {
    // 若 1..len 全部为数组元素且无其他键 → 数组；否则对象。
    let len = table.len().map_err(|e| e.to_string())?;
    let mut arr = Vec::with_capacity(len as usize);
    let mut obj = serde_json::Map::new();
    let mut is_array = true;

    for pair in table.pairs::<mlua::Value, mlua::Value>() {
        let (k, v) = pair.map_err(|e| e.to_string())?;
        let jv = value_to_json(&v)?;
        match k {
            Value::Integer(i) if i >= 1 && i <= len => {
                if arr.len() < i as usize {
                    arr.resize(i as usize, serde_json::Value::Null);
                }
                arr[i as usize - 1] = jv;
            }
            Value::String(s) => {
                is_array = false;
                obj.insert(s.to_str().map_err(|e| e.to_string())?.to_string(), jv);
            }
            _ => {
                is_array = false;
                obj.insert(k.to_string().map_err(|e| e.to_string())?, jv);
            }
        }
    }

    if is_array {
        Ok(serde_json::Value::Array(arr))
    } else {
        Ok(serde_json::Value::Object(obj))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_arithmetic() {
        let engine = ScriptEngine::new().unwrap();
        assert_eq!(engine.eval("1 + 2").unwrap(), serde_json::json!(3));
        assert_eq!(engine.eval("10 / 4").unwrap(), serde_json::json!(2.5));
    }

    #[test]
    fn evaluates_string_concat() {
        let engine = ScriptEngine::new().unwrap();
        assert_eq!(
            engine.eval("'pulsar' .. ' runtime'").unwrap(),
            serde_json::json!("pulsar runtime")
        );
    }

    #[test]
    fn defines_and_calls_function() {
        let engine = ScriptEngine::new().unwrap();
        let value = engine
            .eval("function add(a, b) return a + b end; return add(3, 4)")
            .unwrap();
        assert_eq!(value, serde_json::json!(7));
    }

    #[test]
    fn eval_returns_last_expression() {
        let engine = ScriptEngine::new().unwrap();
        let value = engine
            .eval("local x = 5\nlocal y = 6\nreturn x * y")
            .unwrap();
        assert_eq!(value, serde_json::json!(30));
    }

    #[test]
    fn converts_table_to_object() {
        let engine = ScriptEngine::new().unwrap();
        let value = engine
            .eval("return { name = 'pulsar', version = 1 }")
            .unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "name": "pulsar", "version": 1 })
        );
    }

    #[test]
    fn converts_table_to_array() {
        let engine = ScriptEngine::new().unwrap();
        let value = engine.eval("return { 'a', 'b', 'c' }").unwrap();
        assert_eq!(value, serde_json::json!(["a", "b", "c"]));
    }

    #[test]
    fn returns_null_for_no_value() {
        let engine = ScriptEngine::new().unwrap();
        assert_eq!(engine.eval("-- nothing").unwrap(), serde_json::Value::Null);
    }

    #[test]
    fn invalid_script_returns_readable_error() {
        let engine = ScriptEngine::new().unwrap();
        let err = engine.eval("this is not lua").unwrap_err();
        assert!(err.to_string().contains("script eval failed"));
    }

    #[test]
    fn function_value_is_not_json_convertible() {
        let engine = ScriptEngine::new().unwrap();
        let err = engine.eval("return function() end").unwrap_err();
        assert!(err.to_string().contains("not JSON-convertible"));
    }

    #[test]
    fn separate_engines_have_isolated_state() {
        let a = ScriptEngine::new().unwrap();
        let b = ScriptEngine::new().unwrap();
        a.eval("shared = 'from-a'").unwrap();
        // b 的 VM 里没有 shared，访问应为 nil → JSON null。
        assert_eq!(b.eval("return shared").unwrap(), serde_json::Value::Null);
    }
}
