export interface RemoteEpisode {
  info?: MediaInfo
  id: string
  sort: number
  title: string
  kind: number
  season: number
}

export interface EpisodeGroup {
  id: string
  name: string
  description: string
  type: number
  group_count: number
  episode_count: number
}

export interface RemoteSubject {
  episode_group?: EpisodeGroup | null
  info?: MediaInfo
  seasons?: Season[]
  provider: string
  id: string
  kind: string
  title: string
  original_title: string
  year: string
  overview: string
  poster: string
  episodes: RemoteEpisode[]
  relations: { id: string; title: string; relation: string }[]
  cached: boolean
}

export interface MediaInfo {
  duration_ms?: number | null
  external_ids?: Record<string, string>
  localized_title: string
  original_title: string
  language: string
  original_language: string
  overview: string
  original_overview: string
  poster: string
  local_poster: string
  backdrop: string
  local_backdrop: string
  air_date: string
  genres: number[]
  countries: string[]
  artist: string
}

export interface Season {
  binding_id: string
  number: number
  remote_number: number
  title: string
  remote_title: string
  custom_title: boolean
  info?: MediaInfo
}
