import { t } from '@/i18n'
import type { ResourceFile, Work } from '@/types'

export interface ResourceFilters {
  query: string
  extension: string
  codec: string
  status: string
  minSize: string
  maxSize: string
  minHeight: string
  maxHeight: string
  minBitrate: string
  maxBitrate: string
}
export const defaultFilters: ResourceFilters = {
  query: '',
  extension: '',
  codec: '',
  status: '',
  minSize: '',
  maxSize: '',
  minHeight: '',
  maxHeight: '',
  minBitrate: '',
  maxBitrate: '',
}
export type ResourceSort =
  | 'name'
  | 'extension'
  | 'size'
  | 'resolution'
  | 'bitrate'
  | 'codec'
  | 'duration'
export function resourceStatus(r: ResourceFile) {
  if (!r.available) return 'offline'
  if (r.unavailable) return 'unavailable'
  if (r.probe?.error) return 'error'
  return r.probe ? 'ready' : 'missing'
}
function inRange(
  value: number | null | undefined,
  min: string,
  max: string,
  unit = 1,
) {
  if (!min && !max) return true
  if (value == null) return false
  return (
    (!min || value >= Number(min) * unit) &&
    (!max || value <= Number(max) * unit)
  )
}
export function filterResources(
  files: ResourceFile[],
  works: Work[],
  f: ResourceFilters,
) {
  const titles = new Map(
    works.map((w) => [w.id, `${w.title} ${w.info?.original_title || ''}`]),
  )
  const query = f.query.trim().toLocaleLowerCase()
  const extensions = f.extension
    .toLowerCase()
    .split(/[,，\s]+/)
    .map((x) => x.replace(/^\./, ''))
    .filter(Boolean)
  return files.filter((r) => {
    const p = r.probe?.error ? null : r.probe
    return (
      (!query ||
        `${r.path} ${r.name} ${titles.get(r.asset?.work_id || '') || ''}`
          .toLocaleLowerCase()
          .includes(query)) &&
      (!extensions.length || extensions.includes(r.extension.toLowerCase())) &&
      (!f.codec ||
        (
          p?.video_codec ||
          (r.kind === 'music' ? p?.audio_tracks?.[0]?.codec : '')
        )?.toLowerCase() === f.codec.trim().toLowerCase()) &&
      (!f.status ||
        (f.status === 'unknown'
          ? !p?.height || !p?.bitrate
          : resourceStatus(r) === f.status)) &&
      inRange(r.signature.size, f.minSize, f.maxSize, 1024 ** 3) &&
      inRange(p?.height, f.minHeight, f.maxHeight) &&
      inRange(p?.bitrate, f.minBitrate, f.maxBitrate, 1e6)
    )
  })
}
export function sortResources(
  files: ResourceFile[],
  by: ResourceSort,
  desc: boolean,
) {
  const value = (r: ResourceFile): number | string | null => {
    const p = r.probe?.error ? null : r.probe
    switch (by) {
      case 'name':
        return r.name
      case 'extension':
        return r.extension
      case 'size':
        return r.signature.size
      case 'resolution':
        return p?.width && p.height ? p.width * p.height : null
      case 'bitrate':
        return p?.bitrate ?? null
      case 'codec':
        return p?.video_codec || p?.audio_tracks?.[0]?.codec || null
      case 'duration':
        return p?.duration ?? null
    }
  }
  return [...files].sort((a, b) => {
    const x = value(a),
      y = value(b)
    if (x === null || y === null)
      return x === y ? a.path.localeCompare(b.path) : x === null ? 1 : -1
    const result =
      typeof x === 'number' && typeof y === 'number'
        ? x - y
        : String(x).localeCompare(String(y), undefined, { numeric: true })
    return (desc ? -result : result) || a.path.localeCompare(b.path)
  })
}
export const operationLabels = {
  get split() {
    return t('common.operations.cueSplit')
  },
  get copy() {
    return t('common.operations.copy')
  },
  get move() {
    return t('common.operations.move')
  },
  get hardlink() {
    return t('common.operations.hardlink')
  },
  get symlink() {
    return t('common.operations.symlink')
  },
  get adopt() {
    return t('common.operations.adopt')
  },
  get rename() {
    return t('common.operations.rename')
  },
}
export const statusLabels: Record<string, string> = {
  get ready() {
    return t('resources.probe.complete')
  },
  get missing() {
    return t('resources.probe.pending')
  },
  get error() {
    return t('resources.probe.failed')
  },
  get unavailable() {
    return t('resources.status.unavailable')
  },
  get offline() {
    return t('resources.status.offline')
  },
}
