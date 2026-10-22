import {
  createSourceFile,
  isCallExpression,
  forEachChild,
  ScriptTarget,
  type Node,
  isExpressionStatement,
  isObjectLiteralExpression,
  isStringLiteral,
  parseJsonText,
} from 'typescript'
import { afterEach, describe, expect, it } from 'vitest'
import { operationLabels } from '@/features/resources/resource-model'
import {
  formatMessage,
  getLocale,
  resolveLocale,
  setLanguagePreference,
  t,
} from '@/i18n'
import { episodePosition } from '@/lib/media'
import { backendMessage } from './backend'
import legacyMessages from './legacy-messages.json'
import en from './en.json'
import enSource from './en.json?raw'
import zh from './zh-CN.json'
import zhSource from './zh-CN.json?raw'

afterEach(() => setLanguagePreference('system'))
describe('localization', () => {
  it('rejects duplicate message keys before JSON parsing can overwrite them', () => {
    for (const [file, text] of [
      ['zh-CN.json', zhSource],
      ['en.json', enSource],
    ]) {
      const source = parseJsonText(file, text)
      const statement = source.statements[0]
      if (
        !isExpressionStatement(statement) ||
        !isObjectLiteralExpression(statement.expression)
      )
        throw new Error(`Expected a message catalog object in ${file}`)
      const keys = statement.expression.properties.map((property) => {
        if (!property.name || !isStringLiteral(property.name))
          throw new Error(`Expected a message key in ${file}`)
        return property.name.text
      })
      expect(keys.length, file).toBe(new Set(keys).size)
    }
  })
  it('uses semantic dotted keys and named interpolation arguments', () => {
    for (const [key, value] of Object.entries(zh)) {
      expect(key).toMatch(/^[a-z][a-zA-Z0-9]*(?:\.[a-z][a-zA-Z0-9]*)+$/)
      expect(value).not.toMatch(/\{value\d+(?:\}|\|)/)
      expect(en[key as keyof typeof en]).not.toMatch(/\{value\d+(?:\}|\|)/)
    }
    for (const entry of legacyMessages) {
      expect(Object.hasOwn(zh, entry.key), entry.template).toBe(true)
      expect(entry.key).toMatch(/^errors\./)
    }
  })
  it('resolves every legacy diagnostic with named parameters', () => {
    for (const entry of legacyMessages) {
      setLanguagePreference(entry.locale === 'en' ? 'zh-CN' : 'en')
      const names = 'parameters' in entry ? (entry.parameters ?? []) : []
      let index = 0
      const values = Object.fromEntries(
        names.map((name) => [name, `sample_${name}`]),
      )
      const source = entry.template.replace(
        /\{\{|\}\}|\{([^{}]*)\}/g,
        (match, name: string | undefined) =>
          name === undefined ? match : values[names[index++]],
      )
      expect(backendMessage(source), entry.key).toBe(
        t(entry.key as keyof typeof zh, values),
      )
    }
  })
  it('uses registered keys and matching parameter names at UI call sites', () => {
    const sources = import.meta.glob('../**/*.{ts,tsx}', {
      query: '?raw',
      import: 'default',
      eager: true,
    })
    for (const [file, text] of Object.entries(sources)) {
      const source = createSourceFile(
        file,
        text as string,
        ScriptTarget.Latest,
        true,
      )
      const visit = (node: Node) => {
        if (
          isCallExpression(node) &&
          node.expression.getText(source) === 't' &&
          node.arguments[0]
        ) {
          const keys: string[] = []
          const findKeys = (child: Node) => {
            if (isStringLiteral(child)) keys.push(child.text)
            forEachChild(child, findKeys)
          }
          findKeys(node.arguments[0])
          for (const key of keys)
            expect(Object.hasOwn(zh, key), `${file}: ${key}`).toBe(true)
          const args = node.arguments[1]
          if (keys.length && (!args || isObjectLiteralExpression(args))) {
            const supplied = (args?.properties ?? [])
              .map((prop) => prop.name?.getText(source))
              .sort()
            for (const key of keys) {
              const template = zh[key as keyof typeof zh].replace(
                /\{\{[^{}]*\}\}/g,
                '',
              )
              const expected = [
                ...new Set(
                  [...template.matchAll(/\{(\w+)(?:\}|\|)/g)].map((m) => m[1]),
                ),
              ].sort()
              expect(supplied, `${file}: ${key}`).toEqual(expected)
            }
          }
        }
        forEachChild(node, visit)
      }
      visit(source)
    }
  })
  it('accepts semantic errors with named parameters and preserves user content', () => {
    setLanguagePreference('en')
    expect(
      backendMessage({
        code: 'errors.remote.fileSizeMismatch',
        params: {
          path: 'D:\\Sample\\file.ts',
          expected: 12,
          actual: 10,
        },
      }),
    ).toBe(
      'File size differs from the record: D:\\Sample\\file.ts (expected 12 bytes, found 10 bytes)',
    )
    expect(
      backendMessage({
        code: 'errors.transfer.remoteCommitFailed',
        params: {
          error: '任务已暂停',
        },
      }),
    ).toBe('Could not commit remote file: Task paused')
    setLanguagePreference('zh-CN')
    expect(backendMessage('errors.backup.checkpointMismatch')).toBe(
      '远端移动备份与检查点不一致，停止替换',
    )
  })
  it('keeps task confirmation distinct from unknown media information', () => {
    setLanguagePreference('en')
    expect(t('tasks.status.needsConfirmation')).toBe('Needs confirmation')
    expect(t('common.values.unknown')).toBe('Unknown')
    setLanguagePreference('zh-CN')
    expect(t('tasks.status.needsConfirmation')).toBe('待确认')
    expect(t('common.values.unknown')).toBe('未知')
  })
  it('localizes CUE warnings without translating user file paths', () => {
    setLanguagePreference('zh-CN')
    expect(
      backendMessage('Database track duration differs from CUE'),
    ).toContain('时长')
    expect(
      backendMessage(
        'Referenced audio missing or outside CUE directory; select manually: D:\\Example\\track.flac',
      ),
    ).toBe('引用音源缺失或位于 CUE 目录外，请手动选择：D:\\Example\\track.flac')
    expect(backendMessage('CUE line 3: Data tracks are unsupported')).toBe(
      'CUE 第 3 行：不支持数据轨',
    )
    setLanguagePreference('en')
    expect(backendMessage('Database track duration differs from CUE')).toBe(
      'Database track duration differs from CUE',
    )
  })
  it('resolves system language and falls back to English for unsupported locales', () => {
    expect(resolveLocale(['zh-HK', 'en-US'])).toBe('zh-CN')
    expect(resolveLocale(['en-GB'])).toBe('en')
    expect(resolveLocale(['ja-JP', 'zh-CN'])).toBe('en')
    expect(resolveLocale([])).toBe('en')
    setLanguagePreference('en')
    expect(getLocale()).toBe('en')
  })
  it('keeps both catalogs complete and their interpolation arguments identical', () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(zh).sort())
    const placeholders = (s: string) =>
      [
        ...new Set(
          [
            ...s.replace(/\{\{[^{}]*\}\}/g, '').matchAll(/\{(\w+)(?:\}|\|)/g),
          ].map((m) => m[1]),
        ),
      ].sort()
    for (const key of Object.keys(zh) as (keyof typeof zh)[]) {
      expect(en[key].trim(), key).not.toBe('')
      expect(placeholders(en[key]), key).toEqual(placeholders(zh[key]))
    }
  })
  it('interpolates values without replacing their contents or interpreting markup', () => {
    expect(
      formatMessage('{name}: {count}', { name: '<b>{count}</b>', count: 3 }),
    ).toBe('<b>{count}</b>: 3')
    expect(formatMessage('{missing}')).toBe('{missing}')
    expect(formatMessage('{{fileExt}} is a naming variable')).toBe(
      '{{fileExt}} is a naming variable',
    )
  })
  it('updates helper labels and music positions without reloading modules', () => {
    setLanguagePreference('zh-CN')
    expect(operationLabels.copy).toBe('复制')
    setLanguagePreference('en')
    expect(operationLabels.copy).toBe('Copy')
    expect(episodePosition({ season: 2, number: 1 }, 'music')).toBe(
      'Disc 2 · Track 01',
    )
    expect(t('common.selection.item', { filename: '虚构标题.ts' })).toBe(
      'Select 虚构标题.ts',
    )
  })
  it('formats English counts without changing Chinese counters or interpolated data', () => {
    setLanguagePreference('en')
    expect(t('organize.scan.fileCount', { count: 0 })).toBe('Scanned 0 files')
    expect(t('organize.scan.fileCount', { count: 1 })).toBe('Scanned 1 file')
    expect(t('organize.scan.fileCount', { count: 2 })).toBe('Scanned 2 files')
    expect(formatMessage('{name}', { name: '{count|file|files}' })).toBe(
      '{count|file|files}',
    )
    setLanguagePreference('zh-CN')
    expect(t('organize.scan.fileCount', { count: 1 })).toBe('已扫描 1 个文件')
  })
  it('localizes stored diagnostics while preserving paths and title parameters', () => {
    setLanguagePreference('en')
    expect(backendMessage('备份文件已变化，拒绝清理')).toBe(
      'Backup file changed; refusing cleanup',
    )
    expect(
      backendMessage(
        '此远程条目已关联到 任务已暂停，请选择该本地条目；需要改季号时在剧集目录编辑',
      ),
    ).toContain('linked to 任务已暂停.')
    expect(backendMessage('文件在预览后发生变化：D:\\作品\\虚构标题.ts')).toBe(
      'File changed after preview: D:\\作品\\虚构标题.ts',
    )
    expect(backendMessage('无法提交远端文件：任务已暂停')).toBe(
      'Could not commit remote file: Task paused',
    )
    expect(backendMessage('模板必须保留 {{fileExt}} 扩展名')).toContain(
      '{{fileExt}}',
    )
    expect(backendMessage('D:\\目录\\文件.ts: 任务已暂停')).toBe(
      'D:\\目录\\文件.ts: Task paused',
    )
    expect(backendMessage('Unknown operating system error')).toBe(
      'Unknown operating system error',
    )
    setLanguagePreference('zh-CN')
    expect(backendMessage('任务已暂停')).toBe('任务已暂停')
  })
})
