import { describe, expect, it } from 'vitest'
import type { Asset, Episode, Season, Work } from '@/types'
import { catalogCollection, catalogSeasons } from './catalog'

const episode = (season: number, number: number, enabled = true) =>
  ({ season, number, enabled }) as Episode
const asset = (season: number, number: number, end = number, work_id = 'w') =>
  ({ work_id, season, number, end }) as Asset
const work = (episodes: Episode[], seasons: Partial<Season>[] = []) =>
  ({ id: 'w', kind: 'tv', episodes, seasons, bindings: [] }) as unknown as Work
describe('catalog season completeness', () => {
  it('counts unique enabled episodes covered by merged files, ignoring duplicate sources and versions', () => {
    const w = work([
      episode(2, 1),
      episode(2, 1),
      episode(2, 2),
      episode(2, 3),
      episode(2, 4, false),
      episode(2, 0, false),
    ])
    const [s] = catalogSeasons(w, [
      asset(2, 1, 2),
      asset(2, 1),
      asset(2, 4),
      asset(1, 3),
      asset(2, 3, 3, 'other'),
    ])
    expect([s.owned, s.total]).toEqual([2, 3])
  })
  it('keeps metadata-only and episode-only seasons and prefers a manually edited name', () => {
    const w = work(
      [episode(0, 1, false), episode(4, 1)],
      [
        { number: 2, title: 'remote', binding_id: 'tmdb' },
        { number: 2, title: 'custom', custom_title: true, binding_id: 'bgm' },
        { number: 3, title: 'future' },
      ],
    )
    const result = catalogSeasons(w, [])
    expect(result.map((s) => [s.number, s.owned, s.total])).toEqual([
      [0, 0, 0],
      [2, 0, 0],
      [3, 0, 0],
      [4, 0, 1],
    ])
    expect(result[1].title).toBe('custom')
    expect(result[1].seasons).toHaveLength(2)
  })
  it('keeps repeated music track numbers separate by disc', () => {
    const w = { ...work([episode(1, 1), episode(2, 1)]), kind: 'music' }
    expect(
      catalogSeasons(w, [asset(2, 1)]).map((s) => [s.owned, s.total]),
    ).toEqual([
      [0, 1],
      [1, 1],
    ])
  })
  it('summarizes complete seasons, keeps missing metadata unknown and excludes disabled seasons', () => {
    const w = work(
      [
        episode(0, 1, false),
        episode(1, 1),
        episode(1, 1),
        episode(1, 2),
        episode(2, 1),
        episode(2, 2),
        episode(4, 1, false),
      ],
      [
        { number: 0, title: 'Disabled specials' },
        { number: 3, title: 'Unconfirmed season' },
        { number: 4, title: 'Disabled season' },
      ],
    )
    const collection = catalogCollection(w, [
      asset(1, 1, 2),
      asset(1, 1),
      asset(2, 1),
      asset(2, 1),
      asset(3, 1),
    ])
    expect(collection.complete).toBe(1)
    expect(collection.seasons.map((s) => [s.number, s.owned, s.total])).toEqual(
      [
        [1, 2, 2],
        [2, 1, 2],
        [3, 0, 0],
      ],
    )
  })
  it('includes enabled specials and never marks empty catalogs complete', () => {
    expect(catalogCollection(work([]), [])).toEqual({
      seasons: [],
      complete: 0,
    })
    const result = catalogCollection(work([episode(0, 1), episode(1, 1)]), [
      asset(0, 1),
      asset(1, 1),
    ])
    expect(result.complete).toBe(2)
    expect(result.seasons.map((s) => s.number)).toEqual([0, 1])
  })
})
