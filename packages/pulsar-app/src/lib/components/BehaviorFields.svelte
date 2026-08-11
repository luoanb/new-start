<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type {
    NeighborhoodPoolPolicy,
    SelectionPolicy,
    SessionBehavior,
    ToolPolicy,
  } from "$lib/types";
  import { t } from "$lib/i18n";
  import Select from "./Select.svelte";

  /**
   * 行为表单控件（选型策略 + 工具策略 + 契约手册），受控组件。
   * 由系统神经元编辑场景复用：外部持有 SessionBehavior，任何字段变更即时回调 onChange。
   */
  let {
    value,
    onChange,
  }: {
    value: SessionBehavior | null;
    onChange?: (b: SessionBehavior) => void;
  } = $props();

  // 邻域池默认配额（对齐后端 NeighborhoodPoolPolicy::default）。
  const DEFAULT_POLICY: NeighborhoodPoolPolicy = {
    existing_downstream: 4,
    new_downstream: 2,
    fill_downstream_shortage: true,
    siblings: 2,
    upstream_depth: 3,
    global_top_weight: 5,
  };

  type BehaviorFormState = {
    selection: "none" | "fixed" | "neighborhood" | "global";
    globalLimit: number;
    tools: "none" | "from_neuron" | "allowlist";
    allowlistText: string;
    insertId: string;
  };

  function emptyBehaviorForm(): BehaviorFormState {
    return {
      selection: "none",
      globalLimit: 7,
      tools: "none",
      allowlistText: "",
      insertId: "",
    };
  }

  function behaviorFromForm(f: BehaviorFormState): SessionBehavior {
    const selection: SelectionPolicy =
      f.selection === "fixed"
        ? "Fixed"
        : f.selection === "neighborhood"
          ? { Neighborhood: { policy: DEFAULT_POLICY } }
          : f.selection === "global"
            ? { Global: { limit: f.globalLimit || 7 } }
            : "None";
    const tools: ToolPolicy =
      f.tools === "from_neuron"
        ? "FromNeuron"
        : f.tools === "allowlist"
          ? {
              Allowlist: f.allowlistText
                .split(",")
                .map((s) => s.trim())
                .filter(Boolean),
            }
          : "None";
    const behavior: SessionBehavior = { selection, tools };
    if (f.insertId.trim()) behavior.insert_id = f.insertId.trim();
    return behavior;
  }

  function formFromBehavior(b?: SessionBehavior | null): BehaviorFormState {
    const f = emptyBehaviorForm();
    if (!b) return f;
    const sel = b.selection;
    if (sel === "Fixed") f.selection = "fixed";
    else if (sel !== "None" && typeof sel === "object" && "Global" in sel) {
      f.selection = "global";
      f.globalLimit = sel.Global.limit;
    } else if (sel !== "None" && typeof sel === "object" && "Neighborhood" in sel) {
      f.selection = "neighborhood";
    } else {
      f.selection = "none";
    }
    const tools = b.tools;
    if (tools === "FromNeuron") f.tools = "from_neuron";
    else if (tools !== "None" && typeof tools === "object" && "Allowlist" in tools) {
      f.tools = "allowlist";
      f.allowlistText = tools.Allowlist.join(", ");
    } else {
      f.tools = "none";
    }
    f.insertId = b.insert_id ?? "";
    return f;
  }

  let form = $state(emptyBehaviorForm());
  let lastValue = $state<SessionBehavior | null>(null);

  const selectionOptions = [
    { value: "none", label: t("neuronEditor.none") },
    { value: "fixed", label: t("neuronEditor.fixed") },
    { value: "neighborhood", label: t("neuronEditor.neighborhood") },
    { value: "global", label: t("neuronEditor.global") },
  ];
  const toolsOptions = [
    { value: "none", label: t("neuronEditor.toolNone") },
    { value: "from_neuron", label: t("neuronEditor.toolFromNeuron") },
    { value: "allowlist", label: t("neuronEditor.toolAllowlist") },
  ];

  // 契约手册目录：启动时从后端加载全部可用 insert id 与用途说明。
  type InsertInfo = { id: string; hint: string };
  let insertCatalog = $state<InsertInfo[]>([]);
  onMount(async () => {
    try {
      insertCatalog = (await invoke<InsertInfo[]>("list_insert_catalog")) ?? [];
    } catch (e) {
      console.error("[behavior-fields] failed to load insert catalog", e);
    }
  });

  // “无” + 可用 id；label 附带一句话用途说明，便于辨识。若当前值为目录外的旧值，也保留在选项中以便展示。
  const insertOptions = $derived.by(() => {
    const opts = [
      { value: "", label: t("neuronEditor.none") },
      ...insertCatalog.map((i) => ({
        value: i.id,
        label: i.hint ? `${i.id} · ${i.hint}` : i.id,
      })),
    ];
    if (form.insertId && !opts.some((o) => o.value === form.insertId)) {
      opts.push({ value: form.insertId, label: form.insertId });
    }
    return opts;
  });

  // 外部 value 引用变化（如保存后刷新）时重建表单；组件内部变更不触发。
  $effect(() => {
    if (value !== lastValue) {
      lastValue = value;
      form = formFromBehavior(value);
    }
  });

  function emit() {
    onChange?.(behaviorFromForm(form));
  }
</script>

<div class="form-grid">
  <label class="field">
    <span>{t("neuronEditor.selection")}</span>
    <Select
      value={form.selection}
      options={selectionOptions}
      onchange={(v) => {
        form.selection = v as typeof form.selection;
        emit();
      }}
    />
  </label>
  {#if form.selection === "global"}
    <label class="field">
      <span>{t("neuronEditor.globalLimit")}</span>
      <input type="number" min="1" max="20" bind:value={form.globalLimit} onchange={emit} />
    </label>
  {/if}
  <label class="field">
    <span>{t("neuronEditor.tools")}</span>
    <Select
      value={form.tools}
      options={toolsOptions}
      onchange={(v) => {
        form.tools = v as typeof form.tools;
        emit();
      }}
    />
  </label>
  {#if form.tools === "allowlist"}
    <label class="field">
      <span>{t("neuronEditor.allowlistHint")}</span>
      <input bind:value={form.allowlistText} onchange={emit} placeholder={t("neuronEditor.allowlistHint")} />
    </label>
  {/if}
  <label class="field">
    <span>{t("neuronEditor.insertId")}</span>
    <Select
      value={form.insertId}
      options={insertOptions}
      onchange={(v) => {
        form.insertId = String(v);
        emit();
      }}
    />
  </label>
</div>

<style>
  .form-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: var(--space-2);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--fs-sm);
  }
  .field input {
    padding: var(--space-1);
    border-radius: var(--radius-sm);
    border: var(--border-width) solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-text);
    font-size: var(--fs-sm);
  }
</style>
