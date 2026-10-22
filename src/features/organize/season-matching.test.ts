import { describe, expect, it } from 'vitest'
import type { Mapping, SourceFile, Work } from '@/types'
import { episodeRanges, seasonMatchSummary } from './season-matching'

const work = {
  id: 'sample',
  episodes: [
    ...[1, 2, 3, 5].map((n) => ({
      id: `a${n}`,
      binding_id: 'a',
      season: 1,
      number: n,
      enabled: true,
    })),
    { id: 'b1', binding_id: 'b', season: 1, number: 1, enabled: true },
    { id: 'b4', binding_id: 'b', season: 1, number: 4, enabled: true },
    { id: 'disabled', binding_id: 'a', season: 1, number: 6, enabled: false },
    { id: 'unassigned', binding_id: 'a', season: 1, number: 0, enabled: true },
    { id: 'second', binding_id: 'a', season: 2, number: 1, enabled: true },
  ],
} as Work
const files = ['merged', 'alternate', 'other'].map((path) => ({
  path,
  media: true,
})) as SourceFile[]
const mapping = (ids: string[], overrides: Partial<Mapping> = {}): Mapping => ({
  source: '',
  work_id: work.id,
  episode_ids: ids,
  extra: false,
  ...overrides,
})

describe('season matching coverage', () => {
  it('counts unique episodes across versions and sources, including merged files', () => {
    const result = seasonMatchSummary(
      work,
      files,
      {
        merged: mapping(['a1', 'a2']),
        alternate: mapping(['b1']),
        other: mapping(['a5', 'disabled', 'unassigned', 'second', 'stale']),
      },
      1,
      '',
    )
    expect(result).toEqual({ total: 5, matched: 3, missing: [3, 4] })
  })
  it('respects season and source and excludes removed files, attachments, extras and other works', () => {
    const result = seasonMatchSummary(
      work,
      [...files, { path: 'subtitle', media: false } as SourceFile],
      {
        merged: mapping(['a1', 'a2'], { extra: true }),
        alternate: mapping(['a3'], { work_id: 'other-work' }),
        removed: mapping(['a5']),
        subtitle: mapping(['a2']),
        other: mapping(['b1', 'b4', 'second']),
      },
      1,
      'a',
    )
    expect(result).toEqual({ total: 4, matched: 0, missing: [1, 2, 3, 5] })
    expect(
      seasonMatchSummary(work, files, { merged: mapping(['second']) }, 2, 'a'),
    ).toEqual({ total: 1, matched: 1, missing: [] })
  })
  it('does not report an empty season as complete or infer missing episodes from gaps', () => {
    expect(seasonMatchSummary(work, files, {}, 1, 'a').missing).toEqual([
      1, 2, 3, 5,
    ])
    expect(seasonMatchSummary(work, files, {}, 0, '')).toEqual({
      total: 0,
      matched: 0,
      missing: [],
    })
    expect(seasonMatchSummary(undefined, files, {}, 1, '').total).toBe(0)
  })
  it('compresses consecutive episode numbers and uses track labels for music', () => {
    expect(episodeRanges([5, 1, 3, 2, 2, 100])).toBe('EP01–EP03、EP05、EP100')
    expect(episodeRanges([1, 2, 4], '曲目 ')).toBe('曲目 01–曲目 02、曲目 04')
    expect(episodeRanges([])).toBe('')
  })
})
