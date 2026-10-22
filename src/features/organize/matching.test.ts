import { describe, expect, it } from 'vitest'
import { commonKeywords } from '@/features/metadata/source-search'
import type { SourceFile, Work } from '@/types'
import { batchMatch, extendRange, numbers, standaloneSources } from './matching'

const file = (name: string, hint = 'episode') =>
  ({
    name,
    path: `\\\\nas\\anime\\[Fictional Orbit][BDRIP]\\${name}`,
    parent: '',
    signature: { size: 1, modified: 1 },
    extension: 'mkv',
    media: true,
    hint,
    probe: null,
  }) as SourceFile
const work = {
  id: 'show',
  episodes: [1, 2, 3].map((n) => ({
    id: String(n),
    binding_id: 'bgm',
    number: n,
    season: 2,
    enabled: true,
  })),
} as Work
describe('filename assistance', () => {
  it('does not use codec or resolution digits as episodes', () =>
    expect(numbers('[Fictional Orbit][01][1080P][H264_FLAC].mkv')).toEqual([1]))
  it('extracts shared useful words', () => {
    const words = commonKeywords([
      file('[Fictional Orbit][01][1080P].mkv'),
      file('[Fictional Orbit][02][1080P].mkv'),
    ])
    expect(words.map((w) => w.text)).toContain('Fictional')
    expect(words.map((w) => w.text)).not.toContain('1080P')
  })
  it('respects the chosen numeric field and leaves special files unassigned', () => {
    const rows = batchMatch(
      [
        file('FICTIONAL_VOYAGE_2.01.mkv'),
        file('[FICTIONAL_VOYAGE_2][OVA01].mkv', 'special'),
      ],
      work,
      2,
      'bgm',
      'number',
      1,
      1,
      0,
    )
    expect(rows).toHaveLength(1)
    expect(rows[0].episode_ids).toEqual(['1'])
  })
  it('does not choose between ambiguous providers', () => {
    expect(
      batchMatch(
        [file('[01].mkv')],
        {
          ...work,
          episodes: [
            ...work.episodes,
            { ...work.episodes[0], id: 'tmdb', binding_id: 'tmdb' },
          ],
        },
        2,
        '',
        'number',
        0,
        1,
        0,
      ),
    ).toEqual([])
  })
  it('only forms a contiguous range', () =>
    expect(extendRange(work, '1', '3')).toEqual(['1', '2', '3']))
  it('matches original or translated episode names and refuses ambiguous titles', () => {
    const w = {
      ...work,
      title: '系列名称',
      episodes: [
        {
          ...work.episodes[0],
          title: '新的旅程',
          info: { original_title: '新しい旅' },
        },
        { ...work.episodes[1], title: '最后一天' },
      ],
    } as Work
    expect(
      batchMatch(
        [file('[系列名称][新しい旅][1080p].mkv')],
        w,
        2,
        '',
        'name',
        0,
        1,
        0,
      )[0].episode_ids,
    ).toEqual(['1'])
    expect(
      batchMatch(
        [file('[新的旅程].mkv')],
        {
          ...w,
          episodes: [...w.episodes, { ...w.episodes[0], id: 'duplicate' }],
        },
        2,
        '',
        'name',
        0,
        1,
        0,
      ),
    ).toEqual([])
    expect(
      batchMatch(
        [file('系列名称.mkv')],
        { ...w, episodes: [{ ...w.episodes[0], title: '系列名称' }] },
        2,
        '',
        'name',
        0,
        1,
        0,
      ),
    ).toEqual([])
    expect(
      batchMatch(
        [file('第12話.mkv')],
        { ...w, episodes: [{ ...w.episodes[0], title: '第12話' }] },
        2,
        '',
        'name',
        0,
        1,
        0,
      ),
    ).toEqual([])
  })
  it('treats an album OP track as music and filters other discs', () => {
    const album = {
      ...work,
      kind: 'music',
      episodes: [
        { ...work.episodes[0], season: 1 },
        { ...work.episodes[0], id: 'disc2', season: 2 },
      ],
    } as Work
    const song = { ...file('01 OP.flac', 'extra'), extension: 'flac' }
    expect(
      batchMatch([song], album, 2, '', 'number', 0, 1, 0)[0].episode_ids,
    ).toEqual(['disc2'])
  })
  it('requires word boundaries for Latin episode titles', () => {
    const w = {
      ...work,
      title: 'Show',
      episodes: [{ ...work.episodes[0], title: 'The End' }],
    } as Work
    expect(
      batchMatch([file('Show - The_End.mkv')], w, 2, '', 'name', 0, 1, 0),
    ).toHaveLength(1)
    expect(
      batchMatch([file('Show - The Endless.mkv')], w, 2, '', 'name', 0, 1, 0),
    ).toEqual([])
    expect(
      batchMatch(
        [file('FICTIONAL_VOYAGE_2.mkv')],
        {
          ...w,
          title: 'FICTIONAL VOYAGE',
          episodes: [{ ...w.episodes[0], title: 'Voyage' }],
        },
        2,
        '',
        'name',
        0,
        1,
        0,
      ),
    ).toEqual([])
  })
})

describe('episode names with parenthetical descriptions', () => {
  const fictionalMystery = {
    ...work,
    title: 'Fictional Mystery',
    episodes: [
      {
        ...work.episodes[0],
        season: 5,
        title: '虚构线索（虚构航班182号班机）',
        info: {
          localized_title: '虚构线索（虚构航班182号班机）',
          original_title: 'Hidden Signal (Fictional Flight 182)',
        },
      },
      {
        ...work.episodes[1],
        number: 7,
        season: 5,
        title: 'A Different Episode',
      },
    ],
  } as Work
  const match = (name: string, w = fictionalMystery, binding = '') =>
    batchMatch([file(name)], w, 5, binding, 'name', 0, 1, 0)

  it.each(['', '.mkv', '.mp4'])(
    "matches the reported filename with extension '%s' by title, ignoring S05E07",
    (extension) => {
      expect(
        match(
          `Fictional.Mystery.S05E07.Hidden.Signal.虚构航班182号.1985${extension}`,
        ).map((m) => m.episode_ids),
      ).toEqual([['1']])
    },
  )

  it.each([
    'Fictional.Mystery.S05E07.Hidden.Signal',
    'Fictional Mystery - 虚构线索.mkv',
    'Fictional.Mystery.Hidden_Signal.1080p.mkv',
  ])('matches a unique main title in either language: %s', (name) => {
    expect(match(name).map((m) => m.episode_ids)).toEqual([['1']])
  })

  it('prefers a complete name and leaves shared main titles unresolved', () => {
    const w = {
      ...fictionalMystery,
      episodes: [
        fictionalMystery.episodes[0],
        {
          ...fictionalMystery.episodes[1],
          title: 'Hidden Signal (Another Flight)',
        },
      ],
    } as Work
    expect(match('Fictional.Mystery.Hidden.Signal.mkv', w)).toEqual([])
    expect(
      match('Fictional.Mystery.Hidden.Signal.Fictional.Flight.182.mkv', w).map(
        (m) => m.episode_ids,
      ),
    ).toEqual([['1']])
    expect(
      match('Fictional.Mystery.Hidden.Signal.Another.Flight.mkv', w).map(
        (m) => m.episode_ids,
      ),
    ).toEqual([['2']])
  })

  it('restricts fallback candidates to the selected season, source and enabled episodes', () => {
    const w = {
      ...fictionalMystery,
      episodes: [
        ...fictionalMystery.episodes,
        { ...fictionalMystery.episodes[0], id: 'other-season', season: 4 },
        { ...fictionalMystery.episodes[0], id: 'disabled', enabled: false },
        {
          ...fictionalMystery.episodes[0],
          id: 'other-provider',
          binding_id: 'tmdb',
        },
      ],
    }
    const name = 'Fictional.Mystery.Hidden.Signal.mkv'
    expect(match(name, w)).toEqual([])
    expect(match(name, w, 'bgm').map((m) => m.episode_ids)).toEqual([['1']])
  })

  it.each(['Part 1', 'Part II', '1', 'II', '上', '下篇', '前編', '第2部分'])(
    'does not discard episode qualifiers: %s',
    (qualifier) => {
      const w = {
        ...fictionalMystery,
        episodes: [
          {
            ...fictionalMystery.episodes[0],
            info: undefined,
            title: `Hidden Signal (${qualifier})`,
          },
        ],
      } as Work
      expect(match('Fictional.Mystery.Hidden.Signal.mkv', w)).toEqual([])
      expect(
        match(`Fictional.Mystery.Hidden.Signal.(${qualifier}).mkv`, w),
      ).toHaveLength(1)
    },
  )

  it.each([
    'Fictional.Mystery.S05E07.Fictional.Flight.182.1985.mkv',
    'Fictional.Mystery.S05E01.虚构航班182号.1985.mkv',
    'Fictional.Mystery.Hidden.Signals.mkv',
    'Fictional.Mystery.1985.mkv',
  ])(
    'does not match only a flight, year, episode number or partial word: %s',
    (name) => {
      expect(match(name)).toEqual([])
    },
  )

  it('normalizes fullwidth parentheses and does not use a series name as the main title', () => {
    const ep = {
      ...fictionalMystery.episodes[0],
      info: undefined,
      title: 'Hidden Signal（Fictional Flight 182）',
    }
    expect(
      match('Fictional.Mystery.Hidden.Signal.mkv', {
        ...fictionalMystery,
        episodes: [ep],
      }),
    ).toHaveLength(1)
    expect(
      match('Fictional.Mystery.S05E07.mkv', {
        ...fictionalMystery,
        episodes: [
          { ...ep, title: 'Fictional Mystery (Fictional Flight 182)' },
        ],
      }),
    ).toEqual([])
  })
})

describe('automatic subtitle filtering', () => {
  const source = (name: string, parent = String.raw`\\nas\share\Show`) => ({
    ...file(name),
    parent,
    path: `${parent}\\${name}`,
    media: /\.(mkv|mp4|flac)$/i.test(name),
  })
  it('hides same-stem and dot-suffix subtitles, keeping orphan subtitles and other files', () => {
    const names = [
      '第01集.mkv',
      '第01集.ass',
      '第01集.zh-CN.srt',
      '第01集.JA.forced.SSA',
      '第01集.zh.sub',
      '第01集.zh.idx',
      '第01集.vtt',
      '第01集_简体.ass',
      '第02集.ass',
      '第01集.nfo',
      'poster.jpg',
    ]
    expect(
      standaloneSources(names.map((n) => source(n))).map((f) => f.name),
    ).toEqual([
      '第01集.mkv',
      '第01集_简体.ass',
      '第02集.ass',
      '第01集.nfo',
      'poster.jpg',
    ])
  })
  it('respects directories, dotted media names, case-insensitive Samba paths and extra mappings', () => {
    const movie = source('Movie.Part.1.MKV')
    const subtitle = source('movie.part.1.ZH.ass', '//NAS/SHARE/SHOW')
    const subdirectory = source(
      'Movie.Part.1.ass',
      String.raw`\\nas\share\Show\Subs`,
    )
    const extra = {
      source: movie.path,
      work_id: 'show',
      episode_ids: [],
      extra: true,
    }
    expect(standaloneSources([movie, subtitle, subdirectory])).toEqual([
      movie,
      subdirectory,
    ])
    expect(
      standaloneSources([movie, subtitle], { [movie.path]: extra }),
    ).toEqual([movie, subtitle])
    expect(standaloneSources([subtitle])).toEqual([subtitle])
  })
})
