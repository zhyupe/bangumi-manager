import { describe, expect, it } from 'vitest'
import type { SourceFile, Work } from '@/types'
import {
  batchMatch,
  directoryExtraMappings,
  standaloneSources,
} from './matching'

const file = (name: string, relative?: string): SourceFile => ({
  path: `D:\\Incoming\\${name}`,
  name,
  parent: 'D:\\Incoming',
  extension: name.split('.').at(-1)!,
  signature: { size: 10, modified: 1 },
  media: /\.(mkv|flac)$/.test(name),
  hint: relative ? 'extra' : 'episode',
  extra_relative_path: relative,
  probe: null,
})
const work = (kind: string): Work =>
  ({
    id: 'fictional',
    kind,
    episodes: [1, 2].map((number) => ({
      id: `ep${number}`,
      number,
      season: 1,
      binding_id: 'source',
      enabled: true,
    })),
  }) as Work

describe('directory extras', () => {
  it.each([
    'tv',
    'movie',
    'music',
  ])('keeps directory extras out of episode matching for %s', (kind) => {
    const main = file(kind === 'music' ? '01.flac' : 'S01E01.mkv')
    const trailer = file('00.mkv', 'Trailers/00.mkv')
    const scan = file('cover.jpg', 'Scans/cover.jpg')
    const mappings = batchMatch(
      [trailer, scan, main],
      work(kind),
      1,
      '',
      'sequence',
      0,
      1,
      0,
    )
    expect(
      mappings.find((mapping) => mapping.source === main.path),
    ).toMatchObject({ extra: false, episode_ids: ['ep1'] })
    expect(mappings.filter((mapping) => mapping.extra)).toEqual(
      directoryExtraMappings([trailer, scan], 'fictional'),
    )
    expect(
      mappings
        .filter((mapping) => mapping.extra)
        .every((mapping) => mapping.episode_ids.length === 0),
    ).toBe(true)
  })
  it('shows subtitles inside extras directories as independent files', () => {
    const media = file('clip.mkv', 'Interviews/clip.mkv')
    const subtitle = file('clip.srt', 'Interviews/clip.srt')
    expect(standaloneSources([media, subtitle])).toEqual([media, subtitle])
  })
  it('does not auto-assign ordinary OP filenames or legacy records', () => {
    const op = { ...file('OP01.mkv'), hint: 'extra' }
    expect(
      directoryExtraMappings([op, file('episode.mkv')], 'fictional'),
    ).toEqual([])
  })
})
