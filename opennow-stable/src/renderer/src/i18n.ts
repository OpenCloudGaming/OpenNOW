import { useMemo, useSyncExternalStore } from "react";

import fallbackTranslations from "../../../../locales/en.json";

type TranslationValue = string | number | boolean | null | undefined;
type TranslationValues = Record<string, TranslationValue>;
type TranslationLeaf = string;
type TranslationTree = { [key: string]: TranslationLeaf | TranslationTree };

const FALLBACK_LOCALE = "en";
const LOCALE_STORAGE_KEY = "opennow.locale";

const localeSources = import.meta.glob<string>("../../../../locales/*.json", {
  query: "?raw",
  import: "default",
  eager: true,
});

const fallbackTree = fallbackTranslations as TranslationTree;
const loadedLocales = new Map<string, TranslationTree>([[FALLBACK_LOCALE, fallbackTree]]);
const listeners = new Set<() => void>();

let activeLocale = FALLBACK_LOCALE;
let activeTranslations = fallbackTree;
let snapshotVersion = 0;

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function getSnapshot(): number {
  return snapshotVersion;
}

function emitChange(): void {
  snapshotVersion += 1;
  for (const listener of listeners) {
    listener();
  }
}

function localeFromPath(path: string): string | null {
  const fileName = path.split("/").pop();
  if (!fileName?.endsWith(".json")) return null;
  return normalizeLocale(fileName.slice(0, -".json".length));
}

function getLocaleSource(locale: string): string | null {
  const normalized = normalizeLocale(locale);
  for (const [path, source] of Object.entries(localeSources)) {
    if (localeFromPath(path) === normalized) {
      return source;
    }
  }
  return null;
}

function readStoredLocale(): string | null {
  try {
    const stored = window.localStorage.getItem(LOCALE_STORAGE_KEY);
    return stored ? normalizeLocale(stored) : null;
  } catch {
    return null;
  }
}

function writeStoredLocale(locale: string): void {
  try {
    window.localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // Ignore storage failures; locale can still apply for this runtime.
  }
}

function normalizeLocale(locale: string): string {
  const trimmed = locale.trim().toLowerCase().replace("_", "-");
  return trimmed.split("-")[0] || FALLBACK_LOCALE;
}

function getBrowserLocaleCandidates(): string[] {
  const languages = Array.isArray(navigator.languages) && navigator.languages.length > 0
    ? navigator.languages
    : [navigator.language];
  return languages
    .filter((locale): locale is string => typeof locale === "string" && locale.trim().length > 0)
    .map(normalizeLocale);
}

function getInitialLocale(): string {
  return readStoredLocale() ?? getBrowserLocaleCandidates()[0] ?? FALLBACK_LOCALE;
}

function parseLocaleJson(locale: string, raw: string): TranslationTree | null {
  if (raw.trim().length === 0) {
    console.warn(`[i18n] Locale "${locale}" is empty; falling back to English.`);
    return null;
  }

  try {
    return JSON.parse(raw) as TranslationTree;
  } catch (error) {
    console.warn(`[i18n] Failed to parse locale "${locale}"; falling back to English.`, error);
    return null;
  }
}

function loadTranslations(locale: string): TranslationTree | null {
  const normalized = normalizeLocale(locale);
  if (normalized === FALLBACK_LOCALE) return fallbackTree;

  const cached = loadedLocales.get(normalized);
  if (cached) return cached;

  const source = getLocaleSource(normalized);
  if (source === null) return null;

  const parsed = parseLocaleJson(normalized, source);
  if (parsed) {
    loadedLocales.set(normalized, parsed);
  }
  return parsed;
}

function setActiveTranslations(locale: string, translations: TranslationTree | null): void {
  const normalized = normalizeLocale(locale);
  activeLocale = normalized;
  activeTranslations = translations ?? fallbackTree;
  document.documentElement.lang = activeLocale;
  emitChange();
}

function readNestedValue(tree: TranslationTree, key: string): string | null {
  let current: TranslationLeaf | TranslationTree | undefined = tree;
  for (const segment of key.split(".")) {
    if (!current || typeof current !== "object") return null;
    current = current[segment];
  }
  return typeof current === "string" ? current : null;
}

function interpolate(template: string, values: TranslationValues): string {
  return template.replace(/\{\{\s*([\w.]+)\s*\}\}/g, (match, token: string) => {
    const value = values[token];
    return value === undefined || value === null ? match : String(value);
  });
}

function resolvePluralKey(key: string, values: TranslationValues): string {
  return typeof values.count === "number" && values.count !== 1 ? `${key}_plural` : key;
}

export function t(key: string, values: TranslationValues = {}): string {
  const resolvedKey = resolvePluralKey(key, values);
  const translation =
    readNestedValue(activeTranslations, resolvedKey) ??
    readNestedValue(activeTranslations, key) ??
    readNestedValue(fallbackTree, resolvedKey) ??
    readNestedValue(fallbackTree, key);

  if (!translation) {
    if (import.meta.env.DEV) {
      console.warn(`[i18n] Missing translation key "${key}".`);
    }
    return key;
  }

  return interpolate(translation, values);
}

export function getLocale(): string {
  return activeLocale;
}

export function getAvailableLocales(): string[] {
  const locales = new Set<string>([FALLBACK_LOCALE]);
  for (const path of Object.keys(localeSources)) {
    const locale = localeFromPath(path);
    if (locale) locales.add(locale);
  }
  return [...locales].sort();
}

export async function setLocale(locale: string): Promise<void> {
  const normalized = normalizeLocale(locale);
  const translations = loadTranslations(normalized);
  writeStoredLocale(normalized);
  setActiveTranslations(normalized, translations);
}

export async function initializeLocale(): Promise<void> {
  const initialLocale = getInitialLocale();
  const translations = loadTranslations(initialLocale);
  setActiveTranslations(initialLocale, translations);
}

export function useTranslation(): {
  locale: string;
  availableLocales: string[];
  setLocale: (locale: string) => Promise<void>;
  t: typeof t;
} {
  const snapshot = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  return useMemo(() => ({
    locale: activeLocale,
    availableLocales: getAvailableLocales(),
    setLocale,
    t,
  }), [snapshot]);
}
