import { describe, expect, it } from 'vitest'
import type { Binding, Work } from '@/types'
import { importAssociation, linkedWorks } from './local-association'

const identity = { provider: 'tmdb', kind: 'tv', id: '910000041' }
const binding: Binding = {
  id: 'binding',
  provider: 'tmdb',
  remote_id: identity.id,
  source_kind: 'tv',
  source_title: 'Fictional Harbor',
  season: 4,
}
const work = (changes: Partial<Work> = {}): Work => ({
  id: 'local',
  title: 'Fictional Harbor',
  kind: 'tv',
  year: '',
  folder_name: 'Fictional Harbor',
  tmdb_id: '',
  parent_id: '',
  overview: '',
  poster: '',
  bindings: [binding],
  episodes: [],
  ...changes,
})

describe('existing catalog associations', () => {
  it('matches the provider and source type as well as the ID', () => {
    const local = work()
    expect(linkedWorks([local], identity)).toEqual([local])
    expect(linkedWorks([local], { ...identity, provider: 'bangumi' })).toEqual(
      [],
    )
    expect(linkedWorks([local], { ...identity, kind: 'movie' })).toEqual([])
    const nfo = work({
      tmdb_id: identity.id,
      bindings: [{ ...binding, provider: 'nfo' }],
    })
    expect(importAssociation([nfo], identity).season).toBe(4)
    expect(linkedWorks([local], { ...identity, id: '' })).toEqual([])
  })
  it('chooses the bound work over an add-season target and retains its season and group', () => {
    const group = {
      id: 'group',
      name: 'Fictional order',
      description: '',
      type: 1,
      group_count: 2,
      episode_count: 12,
    }
    const local = work({ bindings: [{ ...binding, episode_group: group }] })
    const other = work({ id: 'other', bindings: [] })
    const association = importAssociation([other, local], identity, 'other')
    expect(association.work).toBe(local)
    expect(association.season).toBe(4)
    expect(association.binding?.episode_group).toEqual(group)
  })
  it('keeps a previously chosen local media type for a matching source binding', () => {
    const local = work({ kind: 'movie', parent_id: 'series' })
    expect(importAssociation([local], identity).work).toBe(local)
  })
  it('recognizes IDs from NFO imports while respecting media type', () => {
    const local = work({ bindings: [], tmdb_id: identity.id })
    expect(linkedWorks([local], identity)).toEqual([local])
    expect(linkedWorks([local], { ...identity, kind: 'movie' })).toEqual([])
    const music = work({
      kind: 'music',
      bindings: [],
      info: {
        external_ids: { musicbrainz: 'release-id' },
        localized_title: '',
        original_title: '',
        language: '',
        original_language: '',
        overview: '',
        original_overview: '',
        poster: '',
        local_poster: '',
        backdrop: '',
        local_backdrop: '',
        air_date: '',
        genres: [],
        countries: [],
        artist: '',
      },
    })
    expect(
      linkedWorks([music], {
        provider: 'musicbrainz',
        kind: 'music',
        id: 'release-id',
      }),
    ).toEqual([music])
  })
  it('does not choose arbitrarily when multiple unbound records share an ID', () => {
    const locals = ['a', 'b'].map((id) =>
      work({ id, bindings: [], tmdb_id: identity.id }),
    )
    expect(importAssociation(locals, identity).work).toBeUndefined()
    expect(importAssociation(locals, identity, 'b').work?.id).toBe('b')
  })
  it('keeps manual new-season association for a different ID', () => {
    const local = work()
    const next = { ...identity, id: '910000042' }
    expect(importAssociation([local], next).work).toBeUndefined()
    expect(importAssociation([local], next, local.id)).toMatchObject({
      work: local,
      season: 5,
    })
  })
})
