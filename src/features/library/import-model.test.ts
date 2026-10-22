import { describe, expect, it } from 'vitest'
import type { Mapping, Plan, PlanRow } from '@/types'
import { changeImportRows, importReady, mergeImports } from './import-model'

const mapping = (source: string, id: string): Mapping => ({
  source,
  work_id: 'work',
  episode_ids: [id],
  extra: false,
})
const row = (id: string, extra = false): PlanRow => ({
  id,
  source: `D:\\Incoming\\${id}.mkv`,
  target: `D:\\Library\\${id}.mkv`,
  work_id: 'work',
  episode_ids: [id],
  parent_id: null,
  attachment: false,
  extra,
  action: 'copy',
  operation: 'copy',
  warnings: [],
  conflicts: [],
  reviewed: false,
  target_signature: null,
  signature: { size: 1, modified: 1 },
  probe: null,
  target_probe: null,
  state: 'pending',
  error: '',
  stage: '',
  digest: '',
  backups: [],
})
const plan = (...rows: PlanRow[]) =>
  ({
    rows,
    works: [
      {
        id: 'work',
        episodes: rows.map((r, i) => ({ id: r.id, season: 1, number: i + 1 })),
      },
    ],
  }) as Plan

describe('library import drafts', () => {
  it('keeps distinct parts and refuses duplicate parts or whole/part overlaps', () => {
    const a = { ...mapping('D:/part1.mkv', 'a'), part: 1 }
    const b = { ...mapping('D:/part2.mkv', 'a'), part: 2 }
    expect(mergeImports([a], [b])).toEqual([a, b])
    const first = { ...row('a'), part: 1 }
    const second = { ...row('b'), episode_ids: ['a'], part: 2 }
    expect(importReady(plan(first, second))).toBe(true)
    expect(importReady(plan(first, { ...second, part: 1 }))).toBe(false)
    expect(importReady(plan(first, { ...second, part: 0 }))).toBe(false)
  })
  it('replaces a repeated episode drop and keeps other episodes and extras', () => {
    const extra = {
      ...mapping('D:\\Incoming\\cover.jpg', ''),
      extra: true,
      episode_ids: [],
    }
    const first = mapping('D:\\Incoming\\old.mkv', 'ep1')
    const second = mapping('D:\\Incoming\\ep2.mkv', 'ep2')
    const next = mapping('D:\\Incoming\\new.mkv', 'ep1')
    expect(mergeImports([first, second, extra], [next])).toEqual([
      second,
      extra,
      next,
    ])
    expect(
      mergeImports([extra], [{ ...extra, source: 'd:/incoming/cover.jpg' }]),
    ).toHaveLength(1)
  })
  it('requires conflict and warning decisions while allowing skipped collisions', () => {
    const a = row('a'),
      b = { ...row('b'), target: a.target }
    const p = plan(a, b)
    expect(importReady(p)).toBe(false)
    expect(importReady(changeImportRows(p, 'b', { action: 'skip' }))).toBe(true)
    expect(
      importReady(plan({ ...a, target_signature: { size: 3, modified: 1 } })),
    ).toBe(false)
    expect(importReady(plan({ ...a, warnings: ['review'] }))).toBe(false)
    expect(
      importReady(
        plan({ ...a, action: 'replace', warnings: ['review'], reviewed: true }),
      ),
    ).toBe(true)
    expect(importReady(plan({ ...a, action: 'skip' }))).toBe(false)
  })
  it('applies parent choices to attached subtitles without changing independent extras', () => {
    const a = row('a'),
      sub = { ...row('sub'), parent_id: 'a', attachment: true },
      extra = row('extra', true)
    const changed = changeImportRows(plan(a, sub, extra), 'a', {
      action: 'replace',
      operation: 'move',
    })
    expect(changed.rows.map((r) => r.operation)).toEqual([
      'move',
      'move',
      'copy',
    ])
    expect(importReady(changed)).toBe(true)
  })
})
