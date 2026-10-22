import { t } from '@/i18n'
import type { Asset, Season, Work } from '@/types'

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
