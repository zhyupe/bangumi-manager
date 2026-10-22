import type { Asset, Probe, Signature } from './media'

export interface ResourceFile {
  unavailable?: boolean
  path: string
  name: string
  extension: string
  kind: string
  signature: Signature
  asset: Asset | null
  probe: Probe | null
  available: boolean
}

export interface ReplacementCandidate {
  old_path: string
  new_path: string
  reason: string
}

export interface ReplacementRequest {
  old_path: string
  new_path: string
  replace_subtitles: boolean
}

export interface ReplacementTarget {
  paths: string[]
  manual: boolean
}

export interface BackupRecord {
  id: string
  plan_id: string
  created_at: string
  root: string
  entry: {
    original: string
    backup: string
    signature: Signature
    preserve_original: boolean
  }
  signature: Signature
  complete: boolean
  state: string
  error: string
}
