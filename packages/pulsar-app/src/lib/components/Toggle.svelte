<script lang="ts">
  let {
    checked = $bindable(),
    label = "",
    disabled = false,
  }: {
    checked?: boolean;
    label?: string;
    disabled?: boolean;
  } = $props();
</script>

<label class="toggle">
  <input
    type="checkbox"
    class="toggle-input"
    role="switch"
    aria-checked={checked}
    bind:checked
    {disabled}
  />
  <span class="track" aria-hidden="true"><span class="thumb"></span></span>
  {#if label}<span class="label">{label}</span>{/if}
</label>

<style>
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    cursor: pointer;
    white-space: nowrap;
  }
  .toggle-input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .track {
    position: relative;
    width: 34px;
    height: 18px;
    flex-shrink: 0;
    border-radius: var(--radius-full);
    background: var(--color-border);
    transition: background var(--duration-fast) var(--ease-out);
  }
  .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--color-elevated);
    transition: transform var(--duration-fast) var(--ease-out);
  }
  .toggle-input:checked + .track {
    background: var(--color-primary);
  }
  .toggle-input:checked + .track .thumb {
    transform: translateX(16px);
  }
  .toggle-input:focus-visible + .track {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }
  .toggle-input:disabled + .track {
    opacity: 0.5;
    cursor: default;
  }
  .label {
    font-size: var(--fs-xs);
    color: var(--color-text);
  }
</style>
