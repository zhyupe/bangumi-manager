import { useSyncExternalStore } from 'react'
import en from './en.json'
import zh from './zh-CN.json'

export type Locale = 'zh-CN' | 'en'
export type LanguagePreference = 'system' | Locale
export type MessageKey = keyof typeof zh
const storageKey = 'bangumi.uiLanguage'
const listeners = new Set<() => void>()
const pluralRules = {
  en: new Intl.PluralRules('en'),
  'zh-CN': new Intl.PluralRules('zh-CN'),
}
export function resolveLocale(languages: readonly string[]): Locale {
  return languages[0]?.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en'
}
function readPreference(): LanguagePreference {
  if (typeof window === 'undefined') return 'system'
  try {
    const saved = localStorage.getItem(storageKey)
    if (saved === 'zh-CN' || saved === 'en') return saved
  } catch {
    /* Storage may be unavailable in a restricted WebView. */
  }
  return 'system'
}
let preference = readPreference()
export const getLanguagePreference = () => preference
export function getLocale(): Locale {
  return preference === 'system'
    ? typeof window === 'undefined'
      ? 'zh-CN'
      : resolveLocale(
          navigator.languages?.length
            ? navigator.languages
            : [navigator.language],
        )
    : preference
}
function changed() {
  if (typeof document !== 'undefined')
    document.documentElement.lang = getLocale()
  listeners.forEach((listener) => listener())
}
export function setLanguagePreference(value: LanguagePreference) {
  preference = ['system', 'zh-CN', 'en'].includes(value) ? value : 'system'
  try {
    if (typeof window !== 'undefined')
      localStorage.setItem(storageKey, preference)
  } catch {
    /* Keep the current session usable. */
  }
  changed()
}
if (typeof window !== 'undefined') {
  window.addEventListener('languagechange', changed)
  window.addEventListener('storage', (event) => {
    if (event.key === storageKey || event.key === null) {
      preference = readPreference()
      changed()
    }
  })
  changed()
}
const subscribe = (listener: () => void) => {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}
export function useLocale() {
  return useSyncExternalStore(
    subscribe,
    () => `${preference}:${getLocale()}`,
    () => 'system:zh-CN',
  )
}
export function formatMessage(
  template: string,
  values: Record<string, unknown> = {},
  locale: Locale = getLocale(),
) {
  const plural = pluralRules[locale]
  const resolved = template.replace(
    /\{(\w+)\|([^{}|]*)\|([^{}|]*)\}/g,
    (match, key: string, one: string, other: string) =>
      typeof values[key] === 'number' && Number.isFinite(values[key])
        ? plural.select(values[key]) === 'one'
          ? one
          : other
        : match,
  )
  return resolved.replace(/\{(\w+)\}/g, (match, key: string) =>
    Object.hasOwn(values, key) ? String(values[key] ?? '') : match,
  )
}
export function t(key: MessageKey, values?: Record<string, unknown>): string {
  const messages: Partial<Record<MessageKey, string>> =
    getLocale() === 'en' ? en : zh
  return formatMessage(messages[key] ?? zh[key] ?? key, values)
}
export function formatDate(value: string) {
  const date = new Date(value)
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString(getLocale())
}
