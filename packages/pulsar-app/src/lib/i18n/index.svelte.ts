import { en, zh, type Translations } from "./translations";

const STORAGE_KEY = "locale-preference";

type Locale = "zh" | "en";

function initLocale(): Locale {
  if (typeof localStorage === "undefined") return "en";
  return (localStorage.getItem(STORAGE_KEY) as Locale) ?? "en";
}

// Module-level runes for reactive i18n
let currentLocale: Locale = $state(initLocale());

let dict: Translations = $derived(currentLocale === "zh" ? zh : en);

/** Look up a dot-notation key in the current translations dict. */
function lookup(key: string): string {
  const parts = key.split(".");
  let value: unknown = dict;
  for (const part of parts) {
    if (value == null || typeof value !== "object") return key;
    value = (value as Record<string, unknown>)[part];
  }
  return typeof value === "string" ? value : key;
}

/**
 * Get a translated string by dot-notation key.
 * Call this inside a reactive context (template, $derived, $effect)
 * and it will automatically re-evaluate when locale changes.
 */
export function t(key: string, params?: Record<string, string | number>): string {
  let out = lookup(key);
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      out = out.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
    }
  }
  return out;
}

/**
 * Get a translated string from a nested sub-map.
 * `tMap("sidePanel.caps", "chat")` → `"对话"` (zh) or `"Chat"` (en)
 */
export function tMap(prefix: string, subKey: string): string {
  return lookup(`${prefix}.${subKey}`);
}

export function setLocale(l: Locale): void {
  currentLocale = l;
  localStorage.setItem(STORAGE_KEY, l);
}

export function getLocale(): Locale {
  return currentLocale;
}
