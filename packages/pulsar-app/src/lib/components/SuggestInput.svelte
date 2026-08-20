<script lang="ts">
  // SuggestInput：带建议下拉的通用输入框（autocomplete 语义）。
  // - 聚焦 / 输入时触发外部 fetchSuggest（防竞态 + debounce），下拉只展示一层候选
  // - 键盘：↑↓ 循环选择；Enter 选中候选或提交；Tab 补全；Esc 先关下拉再请求关闭
  // - 下拉项单行省略，title 悬浮显示完整内容
  export type SuggestItem = { label: string; value: string; expand?: boolean };

  import Tooltip from "./Tooltip.svelte";

  let {
    value = $bindable(""),
    placeholder = "",
    fetchSuggest,
    onsubmit,
    onclose,
    dropdownWidth,
    focusOnMount = true,
    class: className = "",
  }: {
    value: string;
    placeholder?: string;
    fetchSuggest: (input: string) => Promise<SuggestItem[]>;
    onsubmit?: (value: string) => void;
    onclose?: () => void;
    dropdownWidth?: number;
    focusOnMount?: boolean;
    class?: string;
  } = $props();

  let items = $state<SuggestItem[]>([]);
  let open = $state(false);
  let active = $state(-1);
  let timer: ReturnType<typeof setTimeout> | null = null;
  let seq = 0; // 请求序号：丢弃过期异步结果，防止旧响应覆盖新输入
  let dropdownEl = $state<HTMLDivElement | null>(null);

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
      next = await fetchSuggest(input);
    } catch {
      next = [];
    }
    if (my !== seq) return;
    items = next;
    active = -1;
    open = next.length > 0;
  }

  function scheduleFetch(input: string) {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void runFetch(input), 120);
  }

  // value 变化（用户输入 / 外部程序化赋值）自动拉取建议；挂载首次也会触发
  $effect(() => {
    scheduleFetch(value);
  });

  function select(item: SuggestItem) {
    value = item.value;
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
    // 延迟关闭，避免点击候选（mousedown）被 blur 抢先
    setTimeout(() => {
      open = false;
      active = -1;
    }, 150);
  }

  function focusInput(el: HTMLInputElement) {
    el.focus();
    if (focusOnMount) el.select();
  }
</script>

<div class="suggest-input {className}">
  <input
    class="input"
    {placeholder}
    bind:value
    onfocus={() => void runFetch(value)}
    onkeydown={onKeydown}
    onblur={onBlur}
    use:focusInput
  />
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
          title={item.label}
          onmousedown={(e) => {
            e.preventDefault();
            select(item);
          }}
        >
          {item.label}
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
  .input {
    width: 100%;
    min-width: 0;
    padding: 2px 6px;
    font-size: var(--fs-sm);
    background: var(--color-elevated);
    color: var(--color-text);
    border: 1px solid var(--color-primary);
    border-radius: var(--radius-sm);
    outline: none;
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
  .item:hover,
  .item.active {
    background: var(--color-hover);
  }
</style>
