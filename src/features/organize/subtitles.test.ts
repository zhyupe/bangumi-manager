import { describe, expect, it } from 'vitest'
import { isSubtitle } from '@/lib/subtitles'
import type { SourceFile, Work } from '@/types'
import { batchMatch, numbers } from './matching'
import { seasonMatchSummary } from './season-matching'

const work = {
  id: 'show',
  kind: 'tv',
  title: 'Fictional Voyage',
  episodes: [1, 2, 3].map((n) => ({
    id: `ep-${n}`,
    binding_id: 'b',
    season: 1,
    number: n,
    enabled: true,
    title: `Fictional Chapter ${n}`,
  })),
} as Work
const file = (name: string): SourceFile => ({
  name,
  path: `D:\\Incoming\\${name}`,
  parent: 'D:\\Incoming',
  extension: name.split('.').at(-1)!,
  media: !isSubtitle(name),
  signature: { size: 1, modified: 1 },
  hint: 'episode',
  probe: null,
})
const match = (names: string[], mode = 'number', target = work) =>
  batchMatch(names.map(file), target, 1, 'b', mode, 0, 1, 0)

describe('standalone subtitle matching', () => {
  it('matches each supported subtitle format and ignores language digits', () => {
    for (const ext of ['srt', 'ass', 'ssa', 'sub', 'idx', 'vtt']) {
      expect(match([`Fictional.S01E02.zh-Hans.${ext}`])[0].episode_ids).toEqual(
        ['ep-2'],
      )
    }
    expect(numbers('Fictional.S01E02.zh-123.ass')).toEqual([2])
    expect(numbers('Fictional.S01E02.zh-123&jpn.ass')).toEqual([2])
    expect(numbers('Fictional.S01E02.srt')).toEqual([2])
    expect(numbers('Fictional.01.ass')).toEqual([1])
  })
  it('matches episode titles and movies but does not assign subtitles to music', () => {
    expect(
      match(['Fictional Chapter 2.en.srt'], 'name')[0].episode_ids,
    ).toEqual(['ep-2'])
    expect(
      match(['Subtitles.zh.ass'], 'number', { ...work, kind: 'movie' })[0]
        .episode_ids,
    ).toEqual(['ep-1'])
    expect(
      match(['Fictional.S01E02.zh.ass'], 'number', { ...work, kind: 'music' }),
    ).toEqual([])
  })
  it('sequences subtitle language sets independently from videos and other formats', () => {
    const rows = match(
      [
        'A.mkv',
        'A.zh-Hans.ass',
        'A.en.srt',
        'A.zh-Hans.srt',
        'B.mkv',
        'B.zh-Hans.ass',
        'B.en.srt',
        'B.zh-Hans.srt',
      ],
      'sequence',
    )
    expect(rows).toHaveLength(8)
    for (const row of rows)
      expect(row.episode_ids).toEqual([
        row.source.includes('\\A.') ? 'ep-1' : 'ep-2',
      ])
  })
  it('sequences bilingual sets independently from plain and single-language subtitles', () => {
    const names = ['A', 'B'].flatMap((stem) =>
      ['.ass', '.chs.ass', '.chs&jpn.ass', '.cht&jpn.ass'].map(
        (suffix) => `${stem}${suffix}`,
      ),
    )
    const rows = match(names, 'sequence')
    expect(rows).toHaveLength(names.length)
    for (const row of rows)
      expect(row.episode_ids).toEqual([
        row.source.includes('\\A.') ? 'ep-1' : 'ep-2',
      ])
  })
  it('counts subtitle coverage separately and keeps directory extras outside it', () => {
    const files = [
      file('Fictional.S01E01.zh.ass'),
      file('Fictional.S01E01.en.srt'),
      {
        ...file('Fictional.S01E02.zh.ass'),
        extra_relative_path: 'S01E02.zh.ass',
      },
    ]
    const rows = batchMatch(files, work, 1, 'b', 'number', 0, 1, 0)
    const mappings = Object.fromEntries(rows.map((m) => [m.source, m]))
    expect(mappings[files[2].path].extra).toBe(true)
    expect(seasonMatchSummary(work, files, mappings, 1, 'b').matched).toBe(0)
    expect(seasonMatchSummary(work, files, mappings, 1, 'b', true)).toEqual({
      total: 3,
      matched: 1,
      missing: [2, 3],
    })
  })
})
