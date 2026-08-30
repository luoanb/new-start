<script lang="ts">
  // SuggestInput：带建议下拉的通用输入框（autocomplete 语义）。
  // 两种数据源：
  // - fetchSuggest：异步拉取（聚焦 / 输入时触发，防竞态 + debounce）
  // - suggestions + localFilter：同步全量候选（本地过滤，无需 debounce/fetch）
  // 键盘：↑↓ 循环选择；Enter 选中候选或提交；Tab 补全；Esc 先关下拉再请求关闭
  // 下拉项单行省略，title 悬浮显示完整内容
  export type SuggestItem = { label: string; value: string; expand?: boolean };

  import Tooltip from "./Tooltip.svelte";

  let {
    value = $bindable(""),
    placeholder = "",
    fetchSuggest,
    suggestions,
    localFilter,
    freeSubmit = true,
    onsubmit,
    onchange,
    onclose,
    dropdownWidth,
    focusOnMount = true,
    clearable = false,
    class: className = "",
  }: {
    value: string;
    placeholder?: string;
    fetchSuggest?: (input: string) => Promise<SuggestItem[]>;
    // 同步候选模式：传入则用本地过滤替代异步拉取（本地过滤逻辑由 localFilter 提供）。
    // 允许多个值映射到同一 value 的 label 也走相同 value（用于回填映射）。
    suggestions?: SuggestItem[];
    localFilter?: (items: SuggestItem[], input: string) => SuggestItem[];
    freeSubmit?: boolean;
    onsubmit?: (value: string) => void;
    // 选中候选 / 输入后回车确认的回调（params 为当前 value 文本）。
    onchange?: (value: string) => void;
    onclose?: () => void;
    dropdownWidth?: number;
    focusOnMount?: boolean;
    // 开启后输入框右侧显示 × 清空按钮（点击清空输入并触发 onchange("")）。
    clearable?: boolean;
    class?: string;
  } = $props();

  let items = $state<SuggestItem[]>([]);
  let open = $state(false);
  let active = $state(-1);
  let isFocused = $state(false); // 输入框聚焦状态：下拉展示以它为准
  let timer: ReturnType<typeof setTimeout> | null = null;
  let seq = 0; // 请求序号：丢弃过期异步结果，防止旧响应覆盖新输入
  let dropdownEl = $state<HTMLDivElement | null>(null);

  const isLocal = $derived(!!suggestions);

  // 键盘聚焦（active）行用浮动 Tooltip 展示完整 label（hover 的 title 在键盘导航时不触发）
  const activeEl = $derived(
    dropdownEl && open && active >= 0
      ? (dropdownEl.children[active] as HTMLElement)
      : null,
  );
  const activeText = $derived(active >= 0 ? items[active]?.label ?? "" : "");

  // 键盘循环选择时，保证高亮项始终在下拉可视区内（超出则跟随滚动）
  function scrollActiveIntoView() {
    if (!dropdownEl || active < 0) return;
    const item = dropdownEl.children[active] as HTMLElement | undefined;
    if (!item) return;
    const top = item.offsetTop;
    const bottom = top + item.offsetHeight;
    if (top < dropdownEl.scrollTop) {
      dropdownEl.scrollTop = top;
    } else if (bottom > dropdownEl.scrollTop + dropdownEl.clientHeight) {
      dropdownEl.scrollTop = bottom - dropdownEl.clientHeight;
    }
  }

  $effect(() => {
    if (active >= 0) scrollActiveIntoView();
  });

  async function runFetch(input: string) {
    const my = ++seq;
    let next: SuggestItem[];
    try {
      next = await fetchSuggest!(input);
    } catch {
      next = [];
    }
    if (my !== seq) return;
    items = next;
    active = -1;
    open = isFocused && next.length > 0;
  }

  function scheduleFetch(input: string) {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void runFetch(input), 120);
  }

  // 同步本地候选：聚焦 / 输入即过滤，无需 debounce。默认按 value 匹配，可传 localFilter 覆盖。
  function runLocal(input: string) {
    const q = input.trim().toLowerCase();
    const filtered = localFilter
      ? localFilter(suggestions!, input)
      : suggestions!.filter(
          (s) =>
            s.value.toLowerCase().includes(q) ||
            (s.label ?? "").toLowerCase().includes(q),
        );
    items = filtered;
    active = -1;
    open = isFocused && filtered.length > 0;
  }

  // value 或候选变化（用户输入 / 外部程序化赋值 / 异步候选加载完成）自动刷新候选。
  // runLocal 内读取 suggestions，Svelte 5 会自动追踪该依赖：候选就绪后聚焦中即展开下拉。
  $effect(() => {
    if (isLocal) runLocal(value);
    else scheduleFetch(value);
  });

  function select(item: SuggestItem) {
    value = item.value;
    onchange?.(item.value);
    if (item.expand) {
      // 候选值可能已以 / 结尾（如目录），避免拼接出双斜杠
      void runFetch(item.value.endsWith("/") ? item.value : item.value + "/");
    } else {
      open = false;
      active = -1;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (open) {
        open = false;
        active = -1;
        e.preventDefault();
      } else {
        onclose?.();
      }
      return;
    }
    if (open && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      const n = items.length;
      if (n === 0) return;
      active = e.key === "ArrowDown" ? (active + 1) % n : (active - 1 + n) % n;
      return;
    }
    if (e.key === "Enter") {
      if (open && active >= 0 && active < items.length) {
        e.preventDefault();
        select(items[active]);
        return;
      }
      onsubmit?.(value);
      return;
    }
    if (e.key === "Tab" && open) {
      e.preventDefault();
      const item = items[active >= 0 ? active : 0];
      if (item) select(item);
    }
  }

  function onBlur() {
    isFocused = false;
    // 延迟关闭，避免点击候选（mousedown）被 blur 抢先
    setTimeout(() => {
      open = false;
      active = -1;
    }, 150);
  }

  function onFocus() {
    isFocused = true;
    // 本地候选模式聚焦即渲染候选；异步模式聚焦即拉取。
    if (isLocal) runLocal(value);
    else void runFetch(value);
  }

  function focusInput(el: HTMLInputElement) {
    el.focus();
    if (focusOnMount) el.select();
  }

  // 清空输入：value 置空并重新按当前模式渲染候选（聚焦中保持下拉展示全量候选）。
  function clear() {
    value = "";
    active = -1;
    if (isLocal) runLocal(value);
    else void runFetch(value);
    onchange?.("");
  }
</script>

<div class="suggest-input {className}">
  <div class="input-wrap">
    <input
      class="input"
      {placeholder}
      bind:value
      onfocus={onFocus}
      onkeydown={onKeydown}
      onblur={onBlur}
      use:focusInput
    />
    {#if clearable && value}
      <button
        type="button"
        class="clear"
        title="Clear"
        aria-label="Clear"
        onmousedown={(e) => e.preventDefault()}
        onclick={(e) => {
          e.stopPropagation();
          clear();
        }}
      >
        <svg
          viewBox="0 0 24 24"
          width="12"
          height="12"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    {/if}
  </div>
  {#if open}
    <div
      bind:this={dropdownEl}
      class="dropdown"
      role="listbox"
      aria-label="Suggestions"
      style={dropdownWidth ? `width:${dropdownWidth}px;` : ""}
    >
      {#each items as item, i (item.value)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="item"
          class:active={i === active}
          role="option"
          aria-selected={i === active}
          title={item.value}
          onmousedown={(e) => {
            e.preventDefault();
            select(item);
          }}
        >
          <span class="v">{item.value}</span>
          {#if item.label && item.label !== item.value}
            <span class="sub">{item.label}</span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <Tooltip target={activeEl} content={activeText} />
</div>

<style>
  .suggest-input {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  .input-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }
  .input {
    width: 100%;
    min-width: 0;
    padding: 2px 6px;
    font-size: var(--fs-sm);
    background: var(--color-elevated);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    outline: none;
  }
  .input-wrap:has(.clear) .input {
    padding-right: 22px;
  }
  .clear {
    position: absolute;
    right: 3px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
  }
  .clear:hover {
    color: var(--color-text);
    background: var(--color-hover);
  }
  .input:focus {
    border-color: var(--color-primary);
  }
  .dropdown {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    right: 0; /* 传 dropdownWidth 时被 inline width 覆盖 */
    z-index: 30;
    max-height: 220px;
    overflow-y: auto;
    background: var(--color-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
    padding: 2px;
  }
  .item {
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    user-select: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-sm);
    color: var(--color-text);
  }
  .item .v {
    white-space: nowrap;
  }
  .item .sub {
    margin-left: 8px;
    font-size: var(--fs-xs);
    color: var(--color-text-dim);
    white-space: nowrap;
  }
  .item:hover,
  .item.active {
    background: var(--color-hover);
  }
</style>
