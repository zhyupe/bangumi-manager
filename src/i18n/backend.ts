import legacyMessages from './legacy-messages.json'
import { getLocale, type Locale, type MessageKey, t } from './index'
import zh from './zh-CN.json'

const escape = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const tokens = /\{\{|\}\}|\{([^{}]*)\}/g
interface LegacyMessage {
  key: MessageKey
  locale: Locale
  template: string
  parameters?: string[]
}
// Raw messages persist in older tasks and are still returned by native APIs.
// Keep them as source templates, separate from the semantic translation keys.
const entries = legacyMessages as LegacyMessage[]
const exact = new Map(entries.map((entry) => [entry.template, entry]))
const patterns = entries
  .filter((entry) => entry.parameters?.length)
  .map((entry) => {
    let position = 0,
      pattern = '^',
      weight = 0
    for (const match of entry.template.matchAll(tokens)) {
      const literal = entry.template.slice(position, match.index)
      pattern += escape(literal)
      weight += literal.length
      pattern += match[1] === undefined ? escape(match[0][0]) : '([\\s\\S]*?)'
      position = match.index! + match[0].length
    }
    const tail = entry.template.slice(position)
    return {
      ...entry,
      regex: new RegExp(pattern + escape(tail) + '$'),
      weight: weight + tail.length,
    }
  })
  .sort((a, b) => b.weight - a.weight)

function isErrorKey(value: unknown): value is MessageKey {
  return (
    typeof value === 'string' &&
    value.startsWith('errors.') &&
    Object.hasOwn(zh, value)
  )
}

export function backendMessage(value: unknown, depth = 0): string {
  if (
    depth <= 4 &&
    value &&
    typeof value === 'object' &&
    'code' in value &&
    isErrorKey(value.code)
  ) {
    const parameters =
      'params' in value && value.params && typeof value.params === 'object'
        ? (value.params as Record<string, unknown>)
        : {}
    return t(value.code, {
      ...parameters,
      ...('error' in parameters
        ? { error: backendMessage(parameters.error, depth + 1) }
        : {}),
    })
  }
  const text = value instanceof Error ? value.message : String(value ?? '')
  if (depth > 4 || text.length > 64_000) return text
  if (isErrorKey(text)) return t(text)
  const locale = getLocale()
  const entry = exact.get(text)
  if (entry) return entry.locale === locale ? text : t(entry.key)
  for (const { regex, parameters, key, locale: sourceLocale } of patterns) {
    if (sourceLocale === locale) continue
    const match = regex.exec(text)
    if (!match) continue
    return t(
      key,
      Object.fromEntries(
        parameters!.map((name, index) => [
          name,
          name === 'error'
            ? backendMessage(match[index + 1], depth + 1)
            : match[index + 1],
        ]),
      ),
    )
  }
  if (locale === 'zh-CN') {
    const line = /^CUE line (\d+): (.*)$/s.exec(text)
    if (line)
      return t('errors.cue.line', {
        line: line[1],
        error: backendMessage(line[2], depth + 1),
      })
    const separator = text.indexOf(': ')
    const prefix = exact.get(text.slice(0, separator))
    if (separator > 0 && prefix?.locale === 'en')
      return `${t(prefix.key)}：${text.slice(separator + 2)}`
  }
  // Preserve paths and metadata verbatim; only translate known diagnostic spans.
  if (text.startsWith('Error: '))
    return `Error: ${backendMessage(text.slice(7), depth + 1)}`
  const separator = text.indexOf(': ', 3)
  if (separator > 0 && /^(?:[a-z]:[\\/]|\\\\|\/)/i.test(text)) {
    return (
      text.slice(0, separator + 2) +
      backendMessage(text.slice(separator + 2), depth + 1)
    )
  }
  if (text.includes('\n'))
    return text
      .split('\n')
      .map((line) => backendMessage(line, depth + 1))
      .join('\n')
  return text
}
