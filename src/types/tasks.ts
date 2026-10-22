import type { Work } from './library'
import type { Asset, Probe, Signature } from './media'

export interface Mapping {
  part?: number
  source: string
  work_id: string
  episode_ids: string[]
  extra: boolean
}

export interface PlanRow {
  part?: number
  subtitle?: { asset: Asset; root: string } | null
  rename?: { root: string; asset: Asset | null; phase: string } | null
  replacement?: { old_asset: Asset; root: string; adopt: boolean } | null
  remote?: {
    method: 'smb_copy' | 'smb_move' | 'rename'
    ready: boolean
    sha256: string
  } | null
  id: string
  source: string
  target: string
  work_id: string
  parent_id: string | null
  episode_ids: string[]
  extra: boolean
  attachment: boolean
  signature: Signature
  target_signature: Signature | null
  probe: Probe | null
  target_probe: Probe | null
  conflicts: string[]
  warnings: string[]
  reviewed: boolean
  action: string
  operation?:
    | 'copy'
    | 'move'
    | 'hardlink'
    | 'symlink'
    | 'adopt'
    | 'rename'
    | 'split'
  source_cleanup_pending?: boolean
  backups_prepared?: boolean
  state: string
  error: string
  stage: string
  digest: string
  backups: {
    preserve_original?: boolean
    original: string
    backup: string
    signature: Signature
    probe?: Probe | null
    digest: string
  }[]
}

export interface Plan {
  kind?: string
  id: string
  created_at: string
  state: string
  rows: PlanRow[]
  tv_root: string
  movie_root: string
  extras_root: string
  backup_root: string
  verify_hash: boolean
  works: Work[]
  metadata_errors: string[]
  metadata_notices?: string[]
  cleanup_notices?: string[]
}

export interface QueueState {
  ids: string[]
  paused: boolean
  error: string
}
