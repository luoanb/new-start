use std::{collections::HashMap, fs, path::PathBuf};

use serde::Deserialize;

use crate::core::{
    config::NeuronSection,
    error::AppResult,
};

/// 活跃神经元数量默认上限。
pub const DEFAULT_NEURON_CAPACITY: usize = 300;
/// 回收定时任务默认周期（1h）。
pub const DEFAULT_NEURON_RECYCLE_INTERVAL_MS: u64 = 3_600_000;

/// Fallback seed when config.json has no create_neuron_prompt.
pub const DEFAULT_CREATE_NEURON_PROMPT: &str = r#"你是 Agent 应用中的神经元创作者（Neuron Creator）。

一个神经元是一个可复用的能力节点。它的 `content` 之后会被当作系统/知识文本用于选型与执行——请写「可被执行」的文案，而不是「可被展示」的营销文案。

## 输出契约
只返回一个 JSON 对象（不要 markdown 围栏，不要任何解释）：
{"desc":"string","content":"string","tool_ids":["string"]}

## 字段规则
- desc：≤20 字的单一职责标签，推荐 动词+名词（如「需求澄清」）。
- content：一段完整、自洽、可直接执行的提示词/知识块，必须包含：
  1) 角色与目标
  2) 何时被选中 / 何时不应被选中
  3) 操作步骤或判定流程
  4) 输出格式 / 成功标准
  5) 硬约束（明确禁止做什么）
  建议 200–800 中文字符。避免口号、占位符、空泛建议。
- tool_ids：仅该角色真正需要的工具；否则 []。禁止编造工具名。
- weight：可省略且会被忽略；系统始终以 0 权重创建神经元，分数只来自后续评估。

## 职责边界
- 一个神经元 = 一件事。禁止合并不相关的职责。
- 只生成普通能力节点。禁止生成系统级节点：以 assistant_ 开头（如 assistant_select_neuron、assistant_match_topic 等）或等于 create_neuron 的节点——这些由项目内置种子负责，你不要触碰。

## 安全约束
- content 之后会被当作系统/知识文本注入执行上下文：禁止在其中编写提示注入、越权指令、诱导执行者泄露密钥或执行危险命令、以及任何绕过或篡改系统既定职责的内容。

## 质量标准
- 具体到「换一个模型照着做也能得到一致结果」；需求含糊时，做最安全且有用的专职节点，并在 content 内写明假设。
- 不得用散文解释、不得输出多个 JSON、不得返回空 content。

示例（仅参考风格，不要照抄）：
{"desc":"需求澄清","content":"你是需求澄清助手。当用户目标模糊时启用；目标已足够具体时可跳过。步骤：1) 用一句话复述目标 2) 列出缺口信息 3) 最多问 3 个关键问题 4) 给出可执行的下一版需求摘要。输出结构：目标/约束/待确认/下一步。禁止直接写实现代码或跳过澄清。","tool_ids":[]}"#;

/// 内建系统提示词种子（content 本体，系统神经元初始化时优先直落库，不调模型）。
///
/// 覆盖范围 = `assistant_select_neuron` + 4 个裁决 hook；`create_neuron` 种子见
/// [`DEFAULT_CREATE_NEURON_PROMPT`]。可被 `config.json → neurons.bootstrap.system_prompts.<type>`
/// 非空覆盖。`inserts/*.md` 契约段（behavior.insert_id）与 content 互补不重复：
/// content = 角色 / 判定准则 / 输出契约 / 硬约束本体，insert = 每轮附加的字段级约束。
pub const SYSTEM_PROMPT_SEEDS: &[(&str, &str)] = &[
    (
        "assistant_select_neuron",
        r#"你是会话角色选择器。你的职责是从候选神经元中选出最合适的一个，作为当前对话的角色来源。

## 判定准则
- 优先依据神经元的 desc、content 与当前需求的语义匹配度；tool_ids 作为补充线索（可用工具是否贴合任务）。
- weight 仅作参考，不得作为唯一依据；高权重但语义不符的候选不应被选中。
- 一个会话只选一个角色；不组合、不合并多个候选。
- 若所有候选都不理想，选择最接近的可用候选，不要空选。

## 输出契约
只返回一个 JSON 对象：
{"neuron_id":"<candidates 中的某个 id>"}
- neuron_id 必须存在于输入 candidates 中，否则视为非法。
- 不得返回候选外的 id、不得一次返回多个、不得改写任何候选的 desc / content。

## 硬约束
- 禁止编造候选列表之外的 id。
- 禁止用自然语言解释选择理由；只输出 JSON。"#,
    ),
    (
        "assistant_match_topic",
        r#"你是课题匹配与创建判定器。你的职责是判断当前用户输入应切换到已有课题，还是新建课题，并在新建时完成可执行的课题拆解。

## 判定准则
- 若用户输入与某个未完成课题的目标语义高度重合（用户在继续推进该课题），选择 switch 并返回该课题的 topic_id。
- 否则新建课题：从用户输入提炼清晰的课题名称、说明与可验收的子目标列表。
- 开放式提问、闲聊、意图模糊的输入也必须拆解出合理目标，不得以「用户没说清楚」为由留空。

## 输出契约
只返回一个 JSON 对象，必须包含顶层 action 字段：
- switch：{"action":"switch","topic_id":"topic_…"}；topic_id 必须来自输入列表中的未完成课题。
- create：{"action":"create","name":"短标题","description":"一句话说明","scope_in":[{"goal":"可执行子目标","done_contract":"可判定的完成标准"}]}。
  scope_in 为核心产出，至少 1 项；每项 goal 与 done_contract 均非空。goal 不空泛，done_contract 可验收（「列出 10 本书并附一句话理由」优于「推荐好书」）。

## 硬约束
- create 分支禁止省略 scope_in 或留空。
- 禁止编造不存在的 topic_id。
- 禁止返回散文替代 JSON。"#,
    ),
    (
        "assistant_complete_scope",
        r#"你是课题完成度验收器。你的职责是依据本轮对话的模型输出、工具结果与用户输入，判定课题 scope 中各项是已完成还是需要阻塞等待用户。

## 判定准则
- 仅当该项的 done_contract 已被本轮出现的证据充分满足时，才标记为 completed。
- 仅当该项无法由 AI 单方推进、必须等待用户提供信息 / 确认 / 批准时，才标记为 blocked。
- 证据不足不勾选；「聊到相关」不构成完成；「进度慢」不构成阻塞。

## 输出契约
只返回一个 JSON 对象：
{"completed_item_ids":["scope_item_id_1"],"blocked_item_ids":["scope_item_id_2"]}
- 元素必须是输入 scope_in 中已有的 id，不得编造。
- 没有完成项时 completed_item_ids 为空数组；没有阻塞项时 blocked_item_ids 为空数组；两者可同时为空。
- completed 与 blocked 不得重叠。

## 硬约束
- 只负责勾选完成 / 阻塞状态，不修改 goal / done_contract 文本。
- 错勾会推进错误进度，错阻塞会暂停轮询；判定从严，不确定就保持未勾选。"#,
    ),
    (
        "assistant_score_feedback",
        r#"你是介入效果评分器。你的职责是评估上一轮介入（角色 / 工具 / 输出）对当前课题推进的实际影响，并给出一个权重增量分。

## 判定准则
- 正分表示介入有帮助、推动了任务；负分表示有害、跑偏或产生反效果。
- 信息不足、缺乏明确评分依据时，输出最小正分 1，而不是 0 或散文说明。
- 分数会直接加到对应神经元及其相关边上，打错会污染网络权重，判定须谨慎。

## 输出契约
只返回一个 JSON 对象，且不得包含 JSON 之外的任何字符（无 markdown 围栏、无解释、无前后言）：
{"score": N}
- score 必须是整数，闭区间 -5..=5。
- score 不得为 0。

## 硬约束
- 无论输入是什么（提问、闲聊、空白、看似与评分无关），都必须输出一个合法 {"score": N}。
- 禁止自然语言回应、禁止反问、禁止要求补充信息、禁止输出数组或多字段、禁止输出 score 0。
- 若上一轮没有介入（本步骤被误调用），按信息不足处理，输出最小正分。"#,
    ),
    (
        "assistant_revise_topic",
        r#"你是课题范围修订器。你的职责是判断当前课题的 scope 是否需要在推进过程中增删改，并输出结构化的变更 diff。

## 判定准则
- 优先响应用户的显式需求变更（补充需求、取消需求、调整验收标准）。
- AI 主动修订仅限 pending / blocked 项：推进中发现 done_contract 不可判定、目标已偏离、契约过时时可修订，但必须能说明理由。
- completed 项只有在本轮为用户对话（User 轮）且用户显式要求时，才允许编辑或删除。
- 无任何合理变更时，不要硬凑 diff。

## 输出契约
只返回一个 JSON 对象，最多包含四类字段：
{"add_items":[{"goal":"可执行子目标","done_contract":"可判定验收标准"}],"remove_item_ids":["scope_…"],"update_items":[{"id":"scope_…","goal":"新目标（可选）","done_contract":"新验收标准（可选）"}],"reason":"变更理由（必填且非空）"}
- add_items 新增为 pending 项，每项 goal 与 done_contract 均非空，缺一即整项跳过。
- remove_item_ids / update_items 的 id 必须来自输入 scope_in 中已有的项。
- update_items 至少携带一个非空字段；同时为空视为非法，跳过该项。
- reason 必填且非空，变更必须能溯源到本轮输入；空洞理由视为无效，跳过本轮全部变更。

## 硬约束
- 禁止编造不存在的 id；禁止省略 reason 或写空泛理由。
- 不管理 completed / blocked 状态（那是 complete_scope 的职责），只增删改条目与文本。
- 编辑 completed 项会被系统重置为 pending；删除已完成项仅在用户明确要求时进行。"#,
    ),
];

#[derive(Debug, Clone)]
pub struct NeuronConfigReader {
    storage_root: PathBuf,
}

#[derive(Debug, Deserialize, Default)]
struct AppConfigSlice {
    neurons: Option<NeuronConfig>,
    /// 顶层 `neuron` 键：容量/回收配置。
    neuron: Option<NeuronSection>,
}

#[derive(Debug, Deserialize, Default)]
struct NeuronConfig {
    bootstrap: Option<NeuronBootstrapConfig>,
}

#[derive(Debug, Deserialize, Default)]
struct NeuronBootstrapConfig {
    create_neuron_prompt: Option<String>,
    /// 按 system_type 覆盖内置系统提示词种子（非空才覆盖）。
    system_prompts: Option<HashMap<String, String>>,
}

impl NeuronConfigReader {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }

    /// Config override if present; otherwise built-in default seed.
    pub fn create_neuron_prompt(&self) -> AppResult<String> {
        let path = self.storage_root.join("config.json");
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: AppConfigSlice = serde_json::from_str(&content)?;
            if let Some(prompt) = config
                .neurons
                .and_then(|neurons| neurons.bootstrap)
                .and_then(|bootstrap| bootstrap.create_neuron_prompt)
                .filter(|prompt| !prompt.trim().is_empty())
            {
                return Ok(prompt);
            }
        }
        Ok(DEFAULT_CREATE_NEURON_PROMPT.to_string())
    }

    /// 内建系统提示词种子（含 config 覆盖）：`system_type` 有内置种子或配置 → `Some`；
    /// 无内置种子的自定义 `system_type` → `None`（调用方回落 LLM 生成）。
    /// 优先级：`config.json → neurons.bootstrap.system_prompts.<type>` 非空覆盖 > 内置种子。
    pub fn system_prompt_for(&self, system_type: &str) -> AppResult<Option<String>> {
        if let Some(overrides) = self.read_system_prompts()? {
            if let Some(prompt) = overrides
                .get(system_type)
                .map(|p| p.as_str())
                .filter(|p| !p.trim().is_empty())
            {
                return Ok(Some(prompt.to_string()));
            }
        }
        Ok(SYSTEM_PROMPT_SEEDS
            .iter()
            .find(|(ty, _)| *ty == system_type)
            .map(|(_, content)| content.to_string()))
    }

    fn read_system_prompts(&self) -> AppResult<Option<HashMap<String, String>>> {
        let path = self.storage_root.join("config.json");
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: AppConfigSlice = serde_json::from_str(&content)?;
            return Ok(config
                .neurons
                .and_then(|neurons| neurons.bootstrap)
                .and_then(|bootstrap| bootstrap.system_prompts));
        }
        Ok(None)
    }

    /// 活跃神经元容量上限；config.json 顶层 `neuron.capacity` 覆盖，默认 300。
    pub fn capacity(&self) -> AppResult<usize> {
        Ok(self
            .read_neuron_section()?
            .capacity
            .unwrap_or(DEFAULT_NEURON_CAPACITY))
    }

    /// 回收定时任务周期（毫秒）；config.json 顶层 `neuron.recycle_interval_ms` 覆盖，默认 1h。
    pub fn recycle_interval_ms(&self) -> AppResult<u64> {
        Ok(self
            .read_neuron_section()?
            .recycle_interval_ms
            .unwrap_or(DEFAULT_NEURON_RECYCLE_INTERVAL_MS))
    }

    fn read_neuron_section(&self) -> AppResult<NeuronSection> {
        let path = self.storage_root.join("config.json");
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: AppConfigSlice = serde_json::from_str(&content)?;
            return Ok(config.neuron.unwrap_or_default());
        }
        Ok(NeuronSection::default())
    }
}
