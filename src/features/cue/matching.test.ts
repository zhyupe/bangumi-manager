import { describe, expect, it } from 'vitest'
import type { RemoteSubject } from '@/types'
import { cueChoices } from './matching'
import type { CueAnalysis } from './types'

const cue: CueAnalysis = {
  id: 'cue',
  sheet: { title: 'Example album', performer: '', files: [], rem: {} },
  sources: [],
  warnings: [],
  tracks: [1, 2].map((number) => ({
    number,
    title: `Cue ${number}`,
    performer: 'Example artist',
    duration: 60,
    extension: 'flac',
    segments: [],
  })),
}
const subject = (positions: number[], disc = 1): RemoteSubject => ({
  provider: 'musicbrainz',
  id: 'release',
  kind: 'music',
  title: 'Example album',
  original_title: '',
  year: '',
  overview: '',
  poster: '',
  cached: false,
  relations: [],
  episodes: positions.map((sort) => ({
    id: `disc${disc}-${sort}`,
    season: disc,
    sort,
    title: `Database ${sort}`,
    kind: 0,
  })),
})
describe('CUE track mapping', () => {
  it('maps a complete valid disc by position and preserves CUE fallback artists', () => {
    expect(
      cueChoices(cue, subject([2, 1])).map((t) => [
        t.remote_id,
        t.title,
        t.performer,
      ]),
    ).toEqual([
      ['disc1-1', 'Database 1', 'Example artist'],
      ['disc1-2', 'Database 2', 'Example artist'],
    ])
  })
  it('does not truncate or cross-match other discs', () => {
    for (const s of [subject([1]), subject([1, 2, 3]), subject([1, 2], 2)])
      expect(cueChoices(cue, s).map((t) => t.remote_id)).toEqual(['', ''])
  })
  it('requires manual mapping for duplicate or discontinuous positions', () => {
    for (const positions of [
      [1, 1],
      [1, 3],
      [0, 1],
      [1, 1.5],
    ])
      expect(
        cueChoices(cue, subject(positions)).every((t) => !t.remote_id),
      ).toBe(true)
  })
  it('keeps local titles and every track when no database is available', () => {
    expect(cueChoices(cue).map((t) => t.title)).toEqual(['Cue 1', 'Cue 2'])
  })
})
