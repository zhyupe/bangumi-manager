import { t } from '@/i18n'
import { natural } from '@/lib/format'
import type { Asset, Season, Work } from '@/types'

export const catalogSorts = ['name', 'release', 'added'] as const
export type CatalogSort = (typeof catalogSorts)[number]

function dateValue(value?: string): number | null {
  if (!value?.trim()) return null
  const time = Date.parse(value)
  return Number.isFinite(time) ? time : null
}

export function sortCatalog(
  works: Work[],
  sort: CatalogSort,
  descending: boolean,
) {
  return [...works].sort((a, b) => {
    const names = natural(a.title, b.title) || natural(a.id, b.id)
    if (sort === 'name') return descending ? -names : names
    const date = (w: Work) =>
      sort === 'added'
        ? dateValue(w.created_at)
        : (dateValue(w.info?.air_date) ??
          dateValue(/^\d{4}$/.test(w.year) ? `${w.year}-01-01` : ''))
    const left = date(a),
      right = date(b)
    // Unknown dates stay last in both directions; ties remain deterministic.
    if (left === null) return right === null ? names : 1
    if (right === null) return -1
    return (descending ? right - left : left - right) || names
  })
}

export function airingSeason(work: Work, today: string): number | null {
  if (work.kind !== 'tv') return null
  const latest = Math.max(
    0,
    ...(work.seasons || []).map((s) => s.number),
    ...work.episodes.map((e) => e.season),
  )
  if (!latest) return null
  // Use regular episode schedules, excluding specials and custom episode orders.
  const episodes = work.episodes.filter(
    (e) =>
      e.season === latest &&
      e.kind === 0 &&
      e.number > 0 &&
      !work.bindings.some((b) => b.id === e.binding_id && b.episode_group),
  )
  const dates = episodes
    .map((e) => e.info?.air_date || '')
    .filter((d) => /^\d{4}-\d{2}-\d{2}$/.test(d) && dateValue(d) !== null)
  return dates.some((d) => d <= today) &&
    dates.some((d) => d >= today) &&
    dates.length > 1
    ? latest
    : null
}

export function catalogSeasons(work: Work, assets: Asset[]) {
  const numbers = new Set([
    ...(work.seasons || []).map((s) => s.number),
    ...work.episodes.map((e) => e.season),
  ])
  return [...numbers]
    .sort((a, b) => a - b)
    .map((number) => {
      const seasons = (work.seasons || []).filter((s) => s.number === number)
      const named =
        seasons.find((s) => s.custom_title && s.title.trim()) ||
        seasons.find((s) => s.title.trim())
      const title =
        named?.title ||
        work.bindings.find(
          (b) => b.provider === 'bangumi' && b.season === number,
        )?.source_title ||
        (number === 0 && work.kind !== 'music' ? t('common.media.special') : '')
      // One episode may have several database entries or several local file versions.
      const expected = new Set(
        work.episodes
          .filter((e) => e.enabled && e.number > 0 && e.season === number)
          .map((e) => e.number),
      )
      const local = assets.filter(
        (a) => a.work_id === work.id && a.season === number,
      )
      const owned = [...expected].filter((n) =>
        local.some((a) => a.number <= n && n <= a.end),
      ).length
      return { number, title, seasons, owned, total: expected.size }
    })
}
export function seasonLabel(number: number, kind: string) {
  return kind === 'music'
    ? `Disc ${String(number).padStart(2, '0')}`
    : `Season ${String(number).padStart(2, '0')}`
}
export function catalogCollection(work: Work, assets: Asset[]) {
  const seasons = catalogSeasons(work, assets).filter(
    (season) =>
      season.total > 0 ||
      !work.episodes.some((episode) => episode.season === season.number),
  )
  const complete = seasons.filter(
    (season) => season.total > 0 && season.owned === season.total,
  ).length
  return { seasons, complete }
}
export function seasonKey(season: Season) {
  return `${season.binding_id}:${season.remote_number}`
}
