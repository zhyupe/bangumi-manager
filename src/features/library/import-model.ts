import { partSlots } from '@/lib/media-parts'
import type { Mapping, Plan, PlanRow } from '@/types'

export type SubmitMode = 'view' | 'execute'
export interface LibraryDrop {
  id: number
  workId: string
  paths: string[]
  episodeId?: string
}
const pathKey = (path: string) => path.replace(/\\/g, '/').toLowerCase()

export function mergeImports(
  current: Mapping[],
  incoming: Mapping[],
): Mapping[] {
  return [
    ...current.filter(
      (old) =>
        !incoming.some(
          (next) =>
            pathKey(old.source) === pathKey(next.source) ||
            (!old.extra &&
              !next.extra &&
              old.work_id === next.work_id &&
              (old.part ?? 0) === (next.part ?? 0) &&
              old.episode_ids.some((id) => next.episode_ids.includes(id))),
        ),
    ),
    ...incoming,
  ]
}

export function changeImportRows(
  plan: Plan,
  id: string,
  changes: Partial<PlanRow>,
): Plan {
  return {
    ...plan,
    rows: plan.rows.map((row) =>
      row.id === id || row.parent_id === id ? { ...row, ...changes } : row,
    ),
  }
}

export function importReady(plan: Plan | null): boolean {
  if (!plan) return false
  const active = plan.rows.filter((row) => row.action !== 'skip')
  if (!active.length) return false
  const targets = new Set<string>()
  const episodes = new Set<string>()
  return active.every((row) => {
    if (
      !['copy', 'replace'].includes(row.action) ||
      (row.warnings.length > 0 && !row.reviewed)
    )
      return false
    if (
      (row.target_signature || row.backups.length) &&
      row.action !== 'replace'
    )
      return false
    const key = pathKey(row.target)
    if (targets.has(key)) return false
    targets.add(key)
    if (!row.extra && !row.attachment) {
      for (const id of row.episode_ids) {
        const ep = plan.works
          .find((w) => w.id === row.work_id)
          ?.episodes.find((e) => e.id === id)
        if (!ep) return false
        for (const part of partSlots(row.part)) {
          const key = `${row.work_id}:${ep.season}:${ep.number}:${part}`
          if (episodes.has(key)) return false
          episodes.add(key)
        }
      }
    }
    return true
  })
}
