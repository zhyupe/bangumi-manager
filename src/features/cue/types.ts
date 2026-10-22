import type { RemoteSubject } from '@/types'
export interface CueSheet {
  title: string
  performer: string
  files: string[]
  rem: Record<string, string>
  references?: { name: string; path: string | null; error: string | null }[]
  tracks?: {
    number: number
    title: string
    performer: string
    index01: number | null
  }[]
}
export interface CueAnalysis {
  id: string
  sheet: CueSheet
  warnings: string[]
  sources: {
    file: { path: string }
    codec: string
    rate: number
    bits: number
    channels: number
  }[]
  tracks: {
    number: number
    title: string
    performer: string
    duration: number
    cut?: {
      start: number
      end: number
      start_offset: number
      end_offset: number
    } | null
    extension: string
    segments: { source: number; start: number; end: number }[]
  }[]
}
export interface CueCandidates {
  method: string
  items: RemoteSubject[]
  next_offset: number | null
}
export interface TrackChoice {
  number: number
  remote_id: string
  title: string
  performer: string
}
