export interface Signature {
  size: number
  modified: number
}

export interface Probe {
  sample_rate?: number | null
  audio_bit_depth?: number | null
  sample_count?: number | null
  version?: number
  container?: string | null
  video_bitrate?: number | null
  bitrate_estimated?: boolean
  profile?: string | null
  pixel_format?: string | null
  bit_depth?: number | null
  frame_rate?: number | null
  color_space?: string | null
  color_transfer?: string | null
  hdr?: string | null
  audio_tracks?: MediaTrack[]
  subtitles?: MediaTrack[]
  duration: number | null
  bitrate: number | null
  width: number | null
  height: number | null
  video_codec: string | null
  audio: string[]
  error: string | null
}

export interface MediaTrack {
  codec: string
  language: string
  title: string
  channels: number | null
}

export interface SourceFile {
  path: string
  name: string
  parent: string
  extension: string
  signature: Signature
  media: boolean
  hint: string
  extra_relative_path?: string | null
  probe: Probe | null
}

export interface Asset {
  part?: number
  path: string
  work_id: string
  season: number
  number: number
  end: number
  signature: Signature
}
