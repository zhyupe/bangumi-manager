import { t } from '@/i18n'
import type { Episode } from '@/types'

export const seasonEpisode = (ep: Pick<Episode, 'season' | 'number'>) =>
  `S${String(ep.season).padStart(2, '0')}E${String(ep.number).padStart(2, '0')}`

export function episodePosition(
  ep: Pick<Episode, 'season' | 'number'>,
  kind = 'tv',
  end = ep.number,
) {
  if (kind === 'movie') return t('common.media.feature')
  const range =
    end > ep.number
      ? `-${kind === 'music' ? '' : 'E'}${String(end).padStart(2, '0')}`
      : ''
  return kind === 'music'
    ? t('music.track.position', {
        disc: ep.season,
        track: String(ep.number).padStart(2, '0'),
        range: range,
      })
    : `${seasonEpisode(ep)}${range}`
}

export const episodeKind = (kind: number) =>
  [t('common.media.mainEpisode'), t('common.media.specialOva'), 'OP', 'ED', t('common.media.trailer'), 'MAD', t('common.media.other')][
    kind
  ] || t('common.media.other')
