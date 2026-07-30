# Design

Visual design system for Agent App — Svelte + Tauri AI Agent desktop client.

## Theme

Dual-theme with equal polish. Light and dark are independently designed, not mechanical inversions.

### Light theme

A warm-leaning neutral palette. Not clinical white, not cold blue-grey.

```
--color-bg:        oklch(0.98 0.003 75)       /* warm off-white background */
--color-surface:   oklch(0.95 0.005 75)       /* subtle warm card surface */
--color-elevated:  oklch(0.92 0.006 75)       /* hover/dropdown elevation */
--color-text:      oklch(0.18 0.008 75)       /* near-black with warm tint */
--color-muted:     oklch(0.55 0.015 75)       /* secondary text */
--color-border:    oklch(0.88 0.006 75)       /* soft warm border */
--color-accent:    oklch(0.55 0.18 255)       /* calm blue accent */
--color-accent-dim: oklch(0.45 0.14 255)      /* darker accent for hover */
--color-on-accent: oklch(0.98 0.002 75)       /* text on accent */
```

### Dark theme

Deep but not pure black. Warm charcoal undertones, avoiding the generic blue-black tool look.

```
--color-bg:        oklch(0.13 0.004 75)       /* deep warm charcoal */
--color-surface:   oklch(0.17 0.005 75)       /* elevated surface */
--color-elevated:  oklch(0.21 0.006 75)       /* overlay/dropdown */
--color-text:      oklch(0.92 0.004 75)       /* off-white with warm tint */
--color-muted:     oklch(0.55 0.01 75)        /* secondary text */
--color-border:    oklch(0.25 0.005 75)       /* subtle border */
--color-accent:    oklch(0.7 0.18 255)        /* lighter blue accent for dark bg */
--color-accent-dim: oklch(0.6 0.15 255)       /* hover state */
--color-on-accent: oklch(0.13 0.004 75)       /* text on accent */
```

Key choice: Both themes share the same hue angle (75 = warm yellowish tint) for neutrals and (255 = blue) for accent. This creates a coherent brand feel across themes without mechanical inversion.

### Semantic colors

```
--color-success:   oklch(0.55 0.15 150)       /* system / tool success */
--color-warning:   oklch(0.65 0.18 85)        /* attention state */
--color-error:     oklch(0.55 0.2 30)         /* error state */
--color-error-bg:  oklch(0.9 0.05 30)         /* light: error background */
--color-error-bg-dark: oklch(0.2 0.05 30)     /* dark: error background */
```

## Typography

System font stack for performance and native fit.

```css
--font-body: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
--font-mono: "SF Mono", "Cascadia Code", "JetBrains Mono", Consolas, monospace;
```

### Scale

| Token | Size | Weight | Line Height | Usage |
|-------|------|--------|-------------|-------|
| `--fs-xs` | 11px | 500 | 1.3 | Labels, timestamps, badges |
| `--fs-sm` | 13px | 400 | 1.4 | Secondary text, metadata |
| `--fs-base` | 14px | 400 | 1.5 | Body text, message content |
| `--fs-lg` | 16px | 500 | 1.4 | Section headings |
| `--fs-xl` | 20px | 600 | 1.3 | Dialog titles, page headers |

Ratio between steps: ~1.25 (musical fourth), ensuring clear hierarchy contrast.

## Spacing

4px base unit. Scale uses 4px increments: 4, 8, 12, 16, 20, 24, 32, 40, 48, 64.

```
--space-1: 4px
--space-2: 8px
--space-3: 12px
--space-4: 16px
--space-5: 20px
--space-6: 24px
--space-8: 32px
--space-10: 40px
--space-12: 48px
--space-16: 64px
```

Spacing is varied intentionally. Chat messages use --space-3 (12px) vertical gap for density; sections use --space-6 (24px) for clear separation.

## Border & Radius

```
--radius-sm: 6px    /* buttons, inputs, small UI elements */
--radius-md: 8px    /* cards, panels, chat bubbles */
--radius-lg: 12px   /* modals, large overlays */
--radius-full: 9999px /* pills, badges, tags */
--border-width: 1px
```

## Layout

Three-column layout with flexible center:

```
┌─────────────────────────────────────────┐
│  Status bar (app name · mode · model)  │ ← 40px
├──────┬──────────────────┬───────────────┤
│      │                  │               │
│ Side │   Chat area      │   Info panel  │
│ bar  │   (messages +    │   (providers  │
│      │    input)        │    · models   │
│      │                  │    · skills)  │
│      │                  │               │
├──────┴──────────────────┴───────────────┤
│  Error banner (conditional)            │
└─────────────────────────────────────────┘
```

- Sidebar: 260px, collapsible at <800px viewport
- Info panel: 280px, hidden at <800px
- Chat area: fills remaining space, max content width ~720px
- Status bar: full width, 40px height
- Navigation: keyboard-first with Tab cycling between sidebar ↔ chat ↔ info

## Components

### Chat bubble

```
┌─────────────────────────────┐
│  You              12:30 PM │ (user, right-aligned, accent bg)
│  What is compaction?       │
└─────────────────────────────┘
┌─────────────────────────────┐
│  Assistant        12:30 PM │ (assistant, left-aligned, surface bg)
│  Compaction is...          │
│                            │
│  It refers to...           │
└─────────────────────────────┘
```

- Max width: 75% of chat area
- Role label + timestamp in small muted text
- User bubble: accent background, right-aligned, bottom-right radius 4px
- Assistant bubble: surface background, left-aligned, bottom-left radius 4px
- System message: centered, full-width, transparent, no bubble

### Session list item

```
┌─────────────────────────────────┐
│ ● Chat   How to use Agent mode │ ← active indicator + mode badge + title
│          3h ago · 12 messages  │ ← timestamp + message count
├─────────────────────────────────┤
│ ○ Agent  Debug tool call #42   │ ← inactive item (no dot)
│          1h ago · 34 messages  │
└─────────────────────────────────┘
```

### Input area

Minimal: textarea + send button. Textarea expands from 1 to 5 lines, then scrolls. Send button disabled when empty or loading.

### Model selector

Dropdown in status bar area. Shows provider + model name compactly. Opens to a list grouped by provider with model capabilities as secondary info.

## Motion

```
--ease-out: cubic-bezier(0.16, 1, 0.3, 1)  /* exponential ease-out */
--duration-fast: 150ms  /* hover, focus, small transitions */
--duration-normal: 250ms /* panel show/hide, modal open */
```

Motion rules:
- Animate only opacity and transform (translate). Never layout properties.
- No bounce, no elastic, no spring.
- Loading indicator: pulsing dot (opacity + scale), not a spinner.
- Messages appear with a subtle fade-in (opacity 0→1, transform translateY(4px)→0, 250ms ease-out).

## Iconography

No custom icons in v1. Use Unicode/text symbols where needed:
- `●` for active session indicator
- `+` for create button
- `×` for dismiss/close
- `⋯` for overflow/more

This avoids icon sprite management and keeps the interface text-native.

## Empty states

```
┌─────────────────────────────────────────┐
│                                         │
│           Start a conversation          │
│                                         │
│      Send a message below to begin      │
│                                         │
│     ┌─────────────────────────────┐     │
│     │  Create a new session +     │     │
│     └─────────────────────────────┘     │
│                                         │
└─────────────────────────────────────────┘
```

Centered, minimal, one action button. No illustration, no mascot.
