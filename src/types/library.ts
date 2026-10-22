import type { Asset } from './media'
import type { EpisodeGroup, MediaInfo, Season } from './metadata'
import type { ResourceFile } from './resources'

export interface Binding {
  episode_group?: EpisodeGroup | null
  id: string
  provider: string
  remote_id: string
  source_kind: string
  source_title: string
  season: number
}

export interface Episode {
  info?: MediaInfo
  id: string
  binding_id: string
  remote_id: string
  sort: number
  title: string
  kind: number
  season: number
  number: number
  enabled: boolean
}

export interface Work {
  info?: MediaInfo
  seasons?: Season[]
  category?: string
  metadata_errors?: string[]
  metadata_notices?: string[]
  id: string
  title: string
  kind: string
  year: string
  folder_name: string
  tmdb_id: string
  parent_id: string
  overview: string
  poster: string
  bindings: Binding[]
  episodes: Episode[]
}

export interface LibraryIndex {
  resources?: ResourceFile[]
  imported?: number
  updated?: number
  assets: Asset[]
  unbound: {
    path: string
    name: string
    kind: string
    files: number
    reason?: string
  }[]
  errors: string[]
  scanned_at: string
  available: boolean
}
