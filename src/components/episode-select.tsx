import { useMemo } from 'react'
import { getLocale, t } from '@/i18n'
import { episodePosition as position } from '@/lib/media'
import type { Episode } from '@/types'
import { ComboboxField } from './combobox-field'

function EpisodeName({
  episode,
  kind,
  end = false,
}: {
  episode: Episode
  kind: string
  end?: boolean
}) {
  const localized = episode.info?.localized_title?.trim() || episode.title
  const original = episode.info?.original_title?.trim()
  return (
    <span className="episode-option">
      <span className="episode-option-title">
        {end ? t('organize.range.endPrefix') : ''}
        {position(episode, kind)} ·{' '}
        {localized || t('organize.episode.untitled')}
      </span>
      <span className="episode-option-original">
        {original
          ? t('library.seasons.originalName', { title: original })
          : t('organize.episode.noOriginalTitle')}
      </span>
    </span>
  )
}
export function EpisodeSelect({
  episodes,
  value,
  onValueChange,
  label,
  disabled,
  kind = 'tv',
  end = false,
}: {
  episodes: Episode[]
  value: string
  onValueChange: (value: string) => void
  label: string
  disabled?: boolean
  kind?: string
  end?: boolean
}) {
  const locale = getLocale()
  const options = useMemo(
    () => [
      {
        value: '',
        label: end
          ? t('organize.file.episode')
          : t('organize.mapping.unmatched'),
        textValue: end
          ? t('organize.file.episode')
          : t('organize.mapping.clear'),
      },
      ...(!end
        ? [
            {
              value: 'extra',
              label: t('organize.mapping.extraOption'),
              textValue: t('organize.mapping.extraOption'),
            },
          ]
        : []),
      ...episodes.map((ep) => ({
        value: ep.id,
        textValue: `${position(ep, kind)} ${kind === 'music' ? t('organize.episode.trackSearchText', { cd: ep.season, disc: ep.season, track: ep.number, number: ep.number }) : kind === 'movie' ? t('organize.episode.movieSearchText') : t('organize.episode.episodeSearchText', { season: ep.season, episode: ep.number, number: ep.number })} ${ep.title} ${ep.info?.localized_title || ''} ${ep.info?.original_title || ''}`,
        label: <EpisodeName episode={ep} kind={kind} end={end} />,
      })),
    ],
    [episodes, kind, end, locale],
  )
  const current = episodes.find((ep) => ep.id === value)
  const selectedContent = end ? (
    current ? (
      t('organize.range.endLabel', { episode: position(current, kind) })
    ) : (
      t('organize.file.episode')
    )
  ) : current ? (
    <EpisodeName episode={current} kind={kind} />
  ) : value === 'extra' ? (
    t('common.files.extra')
  ) : (
    t('organize.mapping.unmatched')
  )
  return (
    <ComboboxField
      label={label}
      searchLabel={
        end
          ? t('organize.range.searchEnd')
          : kind === 'music'
            ? t('organize.episode.searchTrack')
            : kind === 'movie'
              ? t('organize.episode.searchMovie')
              : t('organize.episode.searchEpisode')
      }
      placeholder={
        kind === 'music'
          ? t('organize.episode.trackPlaceholder')
          : kind === 'movie'
            ? t('organize.episode.moviePlaceholder')
            : t('organize.episode.episodePlaceholder')
      }
      emptyMessage={
        kind === 'music'
          ? t('organize.episode.noTracks')
          : kind === 'movie'
            ? t('organize.episode.noMovies')
            : t('organize.episode.noEpisodes')
      }
      className={end ? 'episode-range-select' : 'episode-select'}
      contentClassName="episode-select-content"
      value={value}
      onValueChange={onValueChange}
      disabled={disabled}
      selectedContent={selectedContent}
      options={options}
    />
  )
}
