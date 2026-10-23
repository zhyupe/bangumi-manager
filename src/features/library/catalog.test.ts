import { describe, expect, it } from 'vitest'
import type { Asset, Episode, Season, Work } from '@/types'
import {
  airingSeason,
  catalogCollection,
  catalogSeasons,
  sortCatalog,
} from './catalog'

describe('catalog ordering and airing schedules', () => {
  const item = (id: string, title: string, date = '', created_at = '') =>
    ({
      id,
      title,
      year: '',
      kind: 'tv',
      created_at,
      info: { air_date: date },
      episodes: [],
      seasons: [],
      bindings: [],
    }) as unknown as Work

  it('sorts naturally without mutating the source and keeps missing dates last in either direction', () => {
    const items = [
      item('a', 'Fictional 10', '2025-05-02', '2026-01-02T00:00:00Z'),
      item('b', 'Fictional 2', '2025-05-01', '2026-01-03T00:00:00Z'),
      item('c', 'Fictional 1'),
    ]
    const ids = (sort: 'name' | 'release' | 'added', desc: boolean) =>
      sortCatalog(items, sort, desc).map((w) => w.id)
    expect(ids('name', false)).toEqual(['c', 'b', 'a'])
    expect(ids('name', true)).toEqual(['a', 'b', 'c'])
    expect(ids('release', true)).toEqual(['a', 'b', 'c'])
    expect(ids('release', false)).toEqual(['b', 'a', 'c'])
    expect(ids('added', true)).toEqual(['b', 'a', 'c'])
    expect(ids('added', false)).toEqual(['a', 'b', 'c'])
    expect(items.map((w) => w.id)).toEqual(['a', 'b', 'c'])
  })

  it('falls back to release year and tolerates invalid dates', () => {
    const older = { ...item('a', 'Fictional A', 'invalid'), year: '2024' }
    const newer = item('b', 'Fictional B', '2025-01-01')
    expect(
      sortCatalog([older, newer], 'release', true).map((w) => w.id),
    ).toEqual(['b', 'a'])
  })

  const scheduled = (season: number, number: number, date: string) =>
    ({
      season,
      number,
      kind: 0,
      enabled: true,
      binding_id: 'b',
      info: { air_date: date },
    }) as Episode
  it('includes only the latest regular season during its known broadcast window', () => {
    const w = item('a', 'Fictional Schedule')
    w.episodes = [
      scheduled(1, 1, '2026-01-01'),
      scheduled(2, 1, '2026-10-01'),
      scheduled(2, 2, '2026-10-30'),
      scheduled(0, 1, '2027-01-01'),
    ]
    expect(airingSeason(w, '2026-10-23')).toBe(2)
    expect(airingSeason(w, '2026-10-30')).toBe(2)
    expect(airingSeason(w, '2026-10-31')).toBeNull()
    expect(airingSeason(w, '2026-09-30')).toBeNull()
    expect(airingSeason({ ...w, kind: 'movie' }, '2026-10-23')).toBeNull()
    w.seasons = [{ number: 3 } as any]
    expect(airingSeason(w, '2026-10-23')).toBeNull()
  })

  it('does not infer a schedule from missing dates or custom episode groups', () => {
    const w = item('a', 'Fictional Schedule')
    w.episodes = [scheduled(1, 1, ''), scheduled(1, 2, '2026-10-30')]
    expect(airingSeason(w, '2026-10-23')).toBeNull()
    w.episodes[0].info!.air_date = '2026-10-01'
    w.bindings = [{ id: 'b', episode_group: { id: 'group' } } as any]
    expect(airingSeason(w, '2026-10-23')).toBeNull()
  })
})

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
