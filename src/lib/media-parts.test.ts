import { describe, expect, it } from 'vitest'
import { batchMatch, numbers } from '@/features/organize/matching'
import { subtitleName } from '@/lib/subtitles'
import type { SourceFile, Work } from '@/types'
import { explicitPart, inferParts } from './media-parts'

const file = (name: string, parent = 'D:/Incoming'): SourceFile => ({
  name,
  parent,
  path: `${parent}/${name}`,
  media: !name.endsWith('.ass'),
  extension: name.split('.').at(-1)!,
  hint: 'episode',
  probe: null,
  signature: { size: 1, modified: 1 },
})
const work = {
  id: 'show',
  kind: 'tv',
  title: 'Fictional Voyage',
  episodes: [1, 2].map((n) => ({
    id: `ep${n}`,
    season: 1,
    number: n,
    enabled: true,
    title: `Chapter ${n}`,
  })),
} as Work
const match = (names: string[], mode = 'number', kind = 'tv') =>
  batchMatch(
    names.map((n) => file(n)),
    { ...work, kind },
    1,
    '',
    mode,
    0,
    1,
    0,
  )

describe('multipart media matching', () => {
  it('recognizes explicit parts without treating them as episode numbers', () => {
    for (const marker of ['Part1', 'part 01', 'pt1', 'cd1', 'disc1']) {
      expect(explicitPart(`~Fictional Formula~ ${marker}.mkv`)).toBe(1)
      expect(numbers(`Chapter.S01E02.${marker}.chs.ass`)).toEqual([2])
    }
    expect(explicitPart('Counterpart1.mkv')).toBe(0)
    expect(explicitPart('Chapter.Part10.mkv')).toBe(0)
    expect(explicitPart('Fictional Part1 - part2.mkv')).toBe(2)
    expect(
      match(['Chapter.S01E01.Part1.mkv', 'Chapter.S01E01.Part2.mkv']).map(
        (m) => [m.episode_ids, m.part],
      ),
    ).toEqual([
      [['ep1'], 1],
      [['ep1'], 2],
    ])
  })
  it('infers bare movie suffixes only from consecutive same-directory siblings', () => {
    const files = [
      'Fictional Feature - 01.mkv',
      'Fictional Feature - 02.mkv',
      'Fictional Feature - 03.mkv',
    ].map((n) => file(n))
    expect([...inferParts(files, 'movie').values()]).toEqual([1, 2, 3])
    expect(inferParts(files, 'tv').size).toBe(0)
    expect(inferParts(files.slice(0, 1), 'movie').size).toBe(0)
    expect(inferParts([files[0], files[2]], 'movie').size).toBe(0)
    expect(
      inferParts([files[0], file(files[1].name, 'D:/Other')], 'movie').size,
    ).toBe(0)
    expect(
      match(
        files.map((f) => f.name),
        'number',
        'movie',
      ).map((m) => m.part),
    ).toEqual([1, 2, 3])
  })
  it('keeps parts on the same episode in sequential matching and separates subtitle languages', () => {
    const rows = match(
      [
        'A.Part1.mkv',
        'A.Part2.mkv',
        'B.mkv',
        'A.Part1.chs.ass',
        'A.Part2.chs.ass',
        'B.chs.ass',
      ],
      'sequence',
    )
    expect(rows).toHaveLength(6)
    for (const row of rows)
      expect(row.episode_ids).toEqual([
        row.source.includes('/B.') ? 'ep2' : 'ep1',
      ])
  })
  it('keeps two- and three-letter language codes and script/region qualifiers', () => {
    for (const language of [
      'chs',
      'cht',
      'eng',
      'jpn',
      'zh-Hans',
      'zh-Hans-CN',
      'chs&jpn',
      'CHT&JPN',
      'zh-Hans&ja',
      'chs&jpn&eng',
    ]) {
      expect(subtitleName(`Chapter.E01.${language}.ass`)).toEqual({
        stem: 'Chapter.E01',
        suffix: `.${language.toLowerCase()}.ass`,
      })
      expect(numbers(`Chapter.E01.${language}.ass`)).toEqual([1])
    }
    expect(subtitleName('Chapter.001.ass').stem).toBe('Chapter.001')
    for (const language of ['chs&', '&jpn', 'chs&&jpn', 'chs&123']) {
      const stem = `Chapter.E01.${language}`
      expect(subtitleName(`${stem}.ass`)).toEqual({ stem, suffix: '.ass' })
    }
  })
  it('infers movie parts from bilingual subtitle filenames', () => {
    const rows = match(
      [
        'Fictional Feature - 01.chs&jpn.ass',
        'Fictional Feature - 02.chs&jpn.ass',
      ],
      'number',
      'movie',
    )
    expect(rows.map((row) => [row.episode_ids, row.part])).toEqual([
      [['ep1'], 1],
      [['ep1'], 2],
    ])
  })
  it('matches explicitly numbered parts against a combined episode title', () => {
    const combined = {
      ...work,
      episodes: [
        {
          ...work.episodes[0],
          title: 'Fictional Formula Part1 / Fictional Formula Part2',
        },
      ],
    }
    const rows = batchMatch(
      ['~Fictional Formula~ Part1.mkv', '~Fictional Formula~ Part2.mkv'].map(
        (n) => file(n),
      ),
      combined,
      1,
      '',
      'name',
      0,
      1,
      0,
    )
    expect(rows.map((m) => [m.episode_ids, m.part])).toEqual([
      [['ep1'], 1],
      [['ep1'], 2],
    ])
  })
})
