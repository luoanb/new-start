import { writable, derived, get } from "svelte/store";
import { en, zh, type Translations } from "./translations";

const STORAGE_KEY = "locale-preference";

type Locale = "zh" | "en";

function initLocale(): Locale {
  if (typeof localStorage === "undefined") return "en";
  return (localStorage.getItem(STORAGE_KEY) as Locale) ?? "en";
}

export const locale = writable<Locale>(initLocale());
export const dict = derived<typeof locale, Translations>(locale, ($locale) =>
  $locale === "zh" ? zh : en
);

/** Get a translated string by dot-notation key, e.g. `t("common.send")` */
export function t(key: string): string {
  const d = get(dict);
  const parts = key.split(".");
  let value: unknown = d;
  for (const part of parts) {
    if (value == null || typeof value !== "object") return key;
    value = (value as Record<string, unknown>)[part];
  }
  return typeof value === "string" ? value : key;
}

/** Get a translated string from a nested sub-map by key */
export function tMap(prefix: string, subKey: string): string {
  return t(`${prefix}.${subKey}`);
}

export function setLocale(l: Locale): void {
  locale.set(l);
  localStorage.setItem(STORAGE_KEY, l);
}

export function getLocale(): Locale {
  return get(locale);
}
