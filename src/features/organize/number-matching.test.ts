import { describe, expect, it } from 'vitest'
import type { SourceFile, Work } from '@/types'
import { batchMatch, numbers } from './matching'

const source = (name: string, hint = 'episode'): SourceFile => ({
  name,
  path: `D:\\Incoming\\${name}`,
  parent: 'D:\\Incoming',
  extension: name.split('.').at(-1)!,
  signature: { size: 100, modified: 100 },
  media: true,
  hint,
  probe: null,
})
const work = {
  id: 'fictional-series',
  kind: 'tv',
  episodes: [1, 2, 3].map((number) => ({
    id: `episode-${number}`,
    binding_id: 'source',
    season: 1,
    number,
    enabled: true,
  })),
} as Work

describe('numeric episode matching', () => {
  it.each([
    ['fictionalQe01.ts', [1]],
    ['fictionalQe02.ts', [2]],
    ['Fictional.Series.S01E02.xxx.mkv', [2]],
    ['Fictional.Series.s02e03.1080p.H264.mkv', [3]],
    ['Fictional.Series.S01.E02.mkv', [2]],
    ['Fictional.Series.S01 E02.mkv', [2]],
    ['fictionalQEP003.ts', [3]],
    ['Fictional.Series.Episode 03.mkv', [3]],
    ['[Fictional_Series_2][03][1080p][H264].mkv', [3, 2]],
    ['[Fictional Journey to the Floating Isles! 2][09][720P][GB].mp4', [9, 2]],
    ['[Fictional Journey! 2] [ 09 ] [720P].mp4', [9, 2]],
    ['【架空旅程 2】【09】【720P】.mp4', [9, 2]],
    ['［Fictional Journey ２］［０９］［７２０Ｐ］.mp4', [9, 2]],
    ['[Fictional Journey 2][09][720P].zh-Hans.ass', [9, 2]],
    ['[Fictional Journey 2][E09][720P].mp4', [9, 2]],
    ['Fictional.2.S01E09.[03].mkv', [9, 3, 2]],
    ['Fictional.2.09.mkv', [2, 9]],
  ])('extracts episode fields from %s', (name, expected) => {
    expect(numbers(name)).toEqual(expected)
  })

  it.each([
    'Fictional.Series.2026.1080p.H264.AV1.10bit.mkv',
    'Fictional.Series.S02.H265.720p.mkv',
    'Fictional.Series.ABC123.mkv',
    'Fictional.Series.E1000.ts',
  ])(
    'does not mistake unrelated or partial digits for episodes: %s',
    (name) => {
      expect(numbers(name)).toEqual([])
    },
  )

  it('maps attached and season-prefixed episode numbers, leaving extras for manual matching', () => {
    const files = [
      source('fictionalQe01.ts'),
      source('fictionalQe02.ts'),
      source('Fictional.Series.S01E03.xxx.mkv'),
      source('Fictional.Series.OP01.mkv', 'extra'),
      source('Fictional.Series.OVA01.mkv', 'special'),
    ]
    const matches = batchMatch(files, work, 1, 'source', 'number', 0, 1, 0)
    expect(
      Object.fromEntries(matches.map((m) => [m.source, m.episode_ids])),
    ).toEqual({
      [files[0].path]: ['episode-1'],
      [files[1].path]: ['episode-2'],
      [files[2].path]: ['episode-3'],
    })
  })

  it('keeps explicit numeric position, offset and selected season semantics', () => {
    const target = {
      ...work,
      episodes: work.episodes.map((ep) => ({ ...ep, season: 2 })),
    }
    expect(
      batchMatch(
        [source('Fictional_7.01.mkv')],
        target,
        2,
        'source',
        'number',
        1,
        1,
        1,
      ).map((m) => m.episode_ids),
    ).toEqual([['episode-2']])
    expect(
      batchMatch(
        [source('Fictional.S01E02.mkv')],
        target,
        2,
        'source',
        'number',
        0,
        1,
        0,
      ).map((m) => m.episode_ids),
    ).toEqual([['episode-2']])
  })

  it('leaves missing and ambiguous episodes unassigned', () => {
    expect(
      batchMatch(
        [source('fictionalQe04.ts')],
        work,
        1,
        'source',
        'number',
        0,
        1,
        0,
      ),
    ).toEqual([])
    expect(
      batchMatch(
        [source('fictionalQe02.ts')],
        {
          ...work,
          episodes: [
            ...work.episodes,
            { ...work.episodes[1], id: 'duplicate', binding_id: 'other' },
          ],
        },
        1,
        '',
        'number',
        0,
        1,
        0,
      ),
    ).toEqual([])
  })

  it('uses the bracketed episode even when the title number is also a valid episode', () => {
    const file = source(
      '[Fictional Journey to the Floating Isles! 2][09][720P][GB].mp4',
    )
    const target = {
      ...work,
      episodes: [
        ...work.episodes,
        { ...work.episodes[0], id: 'episode-9', number: 9 },
      ],
    }
    expect(
      batchMatch([file], target, 1, 'source', 'number', 0, 1, 0)[0].episode_ids,
    ).toEqual(['episode-9'])
    expect(batchMatch([file], work, 1, 'source', 'number', 0, 1, 0)).toEqual([])
    expect(
      batchMatch([file], target, 1, 'source', 'number', 1, 1, 0)[0].episode_ids,
    ).toEqual(['episode-2'])
  })
})
