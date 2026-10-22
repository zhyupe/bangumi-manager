import { Link2Icon, ReloadIcon } from '@radix-ui/react-icons'
import { useEffect, useRef, useState } from 'react'
import { Disclosure, DisclosureTrigger } from '@/components/disclosure'
import { Empty } from '@/components/empty-state'
import { Field } from '@/components/field'
import { SelectField, SelectOption } from '@/components/select-field'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableRow } from '@/components/ui/table'
import { t } from '@/i18n'
import { call } from '@/lib/api'
import type { EpisodeGroup, RemoteSubject, Run, Work } from '@/types'
import {
  importAssociation,
  linkedWorks,
  subjectBinding,
} from './local-association'
import { preferredProvider } from './source-search'

type ReleasePage = {
  items: RemoteSubject[]
  total: number
  next_offset: number | null
}

const subjectKey = (subject: RemoteSubject) =>
  JSON.stringify([subject.provider, subject.kind, subject.id])

export default function RemotePane({
  works,
  keyword,
  busy,
  mutationBusy = false,
  episodeGroups = false,
  run,
  onBound,
  initialKind = 'tv',
  existingId = '',
}: {
  works: Work[]
  keyword: string
  busy: boolean
  mutationBusy?: boolean
  episodeGroups?: boolean
  run: Run
  onBound: (w: Work) => Promise<void>
  initialKind?: string
  existingId?: string
}) {
  const [provider, setProvider] = useState(preferredProvider(initialKind)),
    [kind, setKind] = useState(initialKind),
    [query, setQuery] = useState(keyword),
    [directId, setDirectId] = useState('')
  const [results, setResults] = useState<RemoteSubject[]>([]),
    [subject, setSubject] = useState<RemoteSubject>(),
    [existing, setExisting] = useState(''),
    [title, setTitle] = useState(''),
    [season, setSeason] = useState(1),
    [targetKind, setTargetKind] = useState('tv'),
    [tmdb, setTmdb] = useState(''),
    [parent, setParent] = useState('')
  const [groups, setGroups] = useState<EpisodeGroup[]>([])
  const [groupsLoaded, setGroupsLoaded] = useState(false)
  const [groupConfirmed, setGroupConfirmed] = useState(false)
  const [releasePage, setReleasePage] = useState<ReleasePage | null>(null)
  const [releaseQuery, setReleaseQuery] = useState('')
  const [remoteError, setRemoteError] = useState('')
  const [imported, setImported] = useState<
    Record<string, { title: string; updated: boolean }>
  >({})
  const importing = useRef(false)
  const previewRef = useRef<HTMLElement>(null)
  const importedTitle = subject && imported[subjectKey(subject)]
  useEffect(() => {
    previewRef.current?.scrollTo({ top: 0 })
  }, [subject?.provider, subject?.kind, subject?.id])
  const findReleases = async (text: string, offset = 0) => {
    setRemoteError('')
    if (!offset) {
      setSubject(undefined)
      setResults([])
      setReleasePage(null)
    }
    try {
      const page = await call<ReleasePage>('musicbrainz_releases', {
        query: text,
        offset,
        refresh: false,
      })
      setResults((old) =>
        offset
          ? [
              ...old,
              ...page.items.filter(
                (item) => !old.some((p) => p.id === item.id),
              ),
            ]
          : page.items,
      )
      setReleasePage(page)
      setReleaseQuery(text)
    } catch (e) {
      setRemoteError(String(e))
      throw e
    }
  }
  const requestedExisting = works.find((w) => w.id === existingId)
  const existingBinding = requestedExisting?.bindings.find(
    (b) => b.provider === 'tmdb' && b.source_kind === 'tv',
  )
  const autoLoaded = useRef(false)
  useEffect(() => {
    if (keyword) setQuery(keyword)
  }, [keyword])
  const load = (
    id: string,
    source = provider,
    type = kind,
    refresh = false,
    groupId?: string,
  ) =>
    run(t('database.actions.loadDetails'), async () => {
      setRemoteError('')
      if (
        source === 'musicbrainz' &&
        /^https?:\/\/(?:www\.)?musicbrainz\.org\/release-group\//i.test(
          id.trim(),
        )
      ) {
        await findReleases(id)
        return
      }
      if (source === 'musicbrainz') setSubject(undefined)
      try {
        const known = importAssociation(works, {
          provider: source,
          kind: type,
          id: id.trim(),
        })
        let s = await call<RemoteSubject>('media_detail', {
          provider: source,
          id,
          kind: type,
          refresh,
          episodeGroupId: groupId ?? known.binding?.episode_group?.id ?? null,
        })
        const association = importAssociation(
          works,
          s,
          subject && subjectKey(subject) === subjectKey(s)
            ? existing
            : existingId,
        )
        const savedGroup = association.binding?.episode_group?.id
        if (
          groupId === undefined &&
          savedGroup &&
          s.episode_group?.id !== savedGroup
        ) {
          s = await call<RemoteSubject>('media_detail', {
            provider: s.provider,
            id: s.id,
            kind: s.kind,
            refresh,
            episodeGroupId: savedGroup,
          })
        }
        setSubject(s)
        setGroupConfirmed(false)
        if (
          s.id !== subject?.id ||
          s.kind !== subject?.kind ||
          s.provider !== subject?.provider
        ) {
          setGroups([])
          setGroupsLoaded(false)
        }
        setKind(s.kind)
        setProvider(s.provider)
        setTitle(s.original_title || s.title)
        const existingWork = association.work
        setTargetKind(existingWork?.kind || s.kind)
        setTmdb(existingWork?.tmdb_id || (s.provider === 'tmdb' ? s.id : ''))
        setExisting(existingWork?.id || '')
        setParent(existingWork?.parent_id || '')
        if (existingWork) setTitle(existingWork.title)
        setSeason(association.season)
      } catch (e) {
        setRemoteError(String(e))
        throw e
      }
    })
  useEffect(() => {
    if (episodeGroups && existingBinding && !autoLoaded.current) {
      autoLoaded.current = true
      void load(
        existingBinding.remote_id,
        'tmdb',
        'tv',
        false,
        existingBinding.episode_group?.id || '',
      )
    }
  }, [episodeGroups, existingBinding])
  const local = works.find((w) => w.id === existing)
  const matches = subject ? linkedWorks(works, subject) : []
  const updating = matches.some((w) => w.id === existing)
  const binding = local && subject && subjectBinding(local, subject)
  const groupChanged =
    !!binding &&
    (binding.episode_group?.id || '') !== (subject?.episode_group?.id || '')
  const previousEpisodes =
    local?.episodes.filter((e) => e.binding_id === binding?.id) || []
  const retained = previousEpisodes.filter(
    (e) => !subject?.episodes.some((r) => r.id === e.remote_id),
  )
  const moved =
    subject?.episodes.filter((e) => {
      const old = previousEpisodes.find((p) => p.remote_id === e.id)
      return old && (old.season !== e.season || old.number !== e.sort)
    }) || []
  return (
    <div className="remote-panel">
      {remoteError && (
        <Alert variant="destructive" className="remote-error">
          <AlertDescription>{remoteError}</AlertDescription>
        </Alert>
      )}
      <div className="remote-columns">
        <section
          className="remote-search"
          aria-label={t('database.dialog.title')}
        >
          <div className="inline-fields">
            <Field label={t('database.source.label')}>
              <SelectField
                value={provider}
                disabled={busy}
                onValueChange={(value) => {
                  setProvider(value)
                  setResults([])
                  setSubject(undefined)
                  setReleasePage(null)
                  setRemoteError('')
                }}
              >
                <SelectOption value="bangumi">Bangumi</SelectOption>
                <SelectOption value="musicbrainz" disabled={kind !== 'music'}>
                  MusicBrainz
                </SelectOption>
                <SelectOption value="tmdb" disabled={kind === 'music'}>
                  TMDB
                </SelectOption>
              </SelectField>
            </Field>
            <Field label={t('common.fields.type')}>
              <SelectField
                aria-label={t('common.fields.type')}
                value={kind}
                disabled={busy}
                onValueChange={(value) => {
                  setKind(value)
                  setProvider(preferredProvider(value))
                  setSubject(undefined)
                  setResults([])
                  setReleasePage(null)
                  setRemoteError('')
                }}
              >
                <SelectOption value="tv">{t('common.media.series')}</SelectOption>
                <SelectOption value="movie">{t('database.kind.movie')}</SelectOption>
                <SelectOption value="music">{t('database.kind.album')}</SelectOption>
              </SelectField>
            </Field>
          </div>
          <form
            className="search-row"
            onSubmit={(e) => {
              e.preventDefault()
              if (busy || !query.trim()) return
              void run(t('database.search.action'), async () => {
                setRemoteError('')
                if (provider === 'musicbrainz') await findReleases(query)
                else
                  setResults(
                    await call('search_media', { provider, query, kind }),
                  )
              })
            }}
          >
            <Input
              aria-label={t('database.search.label')}
              placeholder={
                provider === 'musicbrainz'
                  ? t('database.search.musicPlaceholder')
                  : t('database.search.workPlaceholder')
              }
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <Button disabled={busy || !query.trim()} type="submit">
              {t('common.actions.search')}
            </Button>
          </form>
          {provider === 'tmdb' && (
            <p className="muted">
              {t(
                'database.search.tmdbHint',
              )}
            </p>
          )}
          {provider === 'musicbrainz' && (
            <p className="muted">
              {t(
                'database.releases.hint',
              )}
            </p>
          )}
          <div className="search-row">
            <Input
              aria-label={t('database.id.label')}
              placeholder={
                provider === 'musicbrainz'
                  ? t('database.id.musicPlaceholder')
                  : t('database.id.tmdbPlaceholder')
              }
              value={directId}
              onChange={(e) => setDirectId(e.target.value)}
            />
            <Button
              variant="secondary"
              disabled={busy || !directId}
              onClick={() => void load(directId)}
            >
              {t('database.id.load')}
            </Button>
          </div>
          {results.length > 0 && (
            <div className="search-results">
              {results.map((item) => (
                <Button
                  key={`${item.provider}-${item.kind}-${item.id}`}
                  className={`result ${subject && subjectKey(subject) === subjectKey(item) ? 'selected' : ''}`}
                  aria-pressed={
                    !!subject && subjectKey(subject) === subjectKey(item)
                  }
                  onClick={() => void load(item.id, item.provider, item.kind)}
                  disabled={busy}
                  variant="ghost"
                >
                  {item.poster ? (
                    <img src={item.poster} alt="" loading="lazy" />
                  ) : (
                    <div className="poster-placeholder">
                      {item.kind === 'music' ? t('database.kind.musicBadge') : t('database.kind.seriesBadge')}
                    </div>
                  )}
                  <span>
                    <strong>{item.original_title || item.title}</strong>
                    {item.title !== item.original_title && (
                      <small>{item.title}</small>
                    )}
                    {item.provider === 'musicbrainz' && (
                      <>
                        <small>{item.info?.artist || t('database.artist.unknown')}</small>
                        <small>{item.overview}</small>
                        {item.cached && (
                          <small>{t('database.cache.offline')}</small>
                        )}
                      </>
                    )}
                    <small>
                      {item.kind === 'movie'
                        ? t('common.media.movie')
                        : item.kind === 'music'
                          ? t('common.media.music')
                          : t('common.media.series')}{' '}
                      · {item.year || t('common.values.unknownYear')} · {item.provider} #
                      {item.id}
                    </small>
                    {imported[subjectKey(item)] && (
                      <small>
                        {imported[subjectKey(item)].updated
                          ? t('database.import.updatedInSession')
                          : t('database.import.addedInSession')}
                      </small>
                    )}
                    {!imported[subjectKey(item)] &&
                      linkedWorks(works, item).length > 0 && (
                        <small>{t('library.status.imported')}</small>
                      )}
                  </span>
                </Button>
              ))}
            </div>
          )}
          {provider === 'musicbrainz' && releasePage && (
            <div className="section-title">
              <p className="muted">
                {releasePage.total
                  ? t('database.releases.loadedCount', {
                      loaded: results.length,
                      total: releasePage.total,
                    })
                  : t('database.releases.empty')}
              </p>
              {releasePage.next_offset != null && (
                <Button
                  variant="secondary"
                  disabled={busy}
                  onClick={() =>
                    void run(t('database.releases.loadMore'), () =>
                      findReleases(releaseQuery, releasePage.next_offset!),
                    )
                  }
                >
                  {t('database.releases.loadMore')}
                </Button>
              )}
            </div>
          )}
        </section>
        <section
          className="remote-preview"
          aria-label={t('database.details.title')}
          ref={previewRef}
        >
          {subject ? (
            <div className="subject-detail">
              <div className="subject-heading">
                <h3>{subject.original_title || subject.title}</h3>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={busy}
                  onClick={() =>
                    void load(
                      subject.id,
                      subject.provider,
                      subject.kind,
                      true,
                      subject.episode_group?.id || '',
                    )
                  }
                >
                  <ReloadIcon />
                  {t('common.actions.refresh')}
                </Button>
              </div>
              <div className="badge-row">
                <Badge>
                  {subject.provider} #{subject.id}
                </Badge>
                <Badge tone="gray">
                  {subject.episodes.length}{' '}
                  {subject.kind === 'music' ? t('database.units.tracks') : t('database.units.chapters')}
                </Badge>
                {subject.cached && (
                  <Badge tone="amber">{t('database.cache.offline')}</Badge>
                )}
              </div>
              {subject.provider === 'musicbrainz' && (
                <div className="musicbrainz-preview">
                  <p>{subject.info?.artist || t('database.artist.unknown')}</p>
                  <p className="muted">{subject.overview}</p>
                  {(subject.seasons || []).map((disc) => (
                    <Disclosure key={disc.number}>
                      <DisclosureTrigger>
                        {t('database.disc.summary', {
                          disc: disc.number,
                          title: disc.title,
                          count: subject.episodes.filter(
                            (e) => e.season === disc.number,
                          ).length,
                        })}
                      </DisclosureTrigger>
                      <Table
                        aria-label={t('database.disc.tracks', {
                          disc: disc.number,
                        })}
                      >
                        <TableBody>
                          {subject.episodes
                            .filter((e) => e.season === disc.number)
                            .map((track) => (
                              <TableRow key={track.id}>
                                <TableCell>
                                  {track.sort === 0
                                    ? t('database.disc.pregap')
                                    : String(track.sort).padStart(2, '0')}
                                </TableCell>
                                <TableCell>{track.title}</TableCell>
                                <TableCell>{track.info?.artist}</TableCell>
                              </TableRow>
                            ))}
                        </TableBody>
                      </Table>
                    </Disclosure>
                  ))}
                </div>
              )}
              {subject.provider === 'tmdb' && subject.kind === 'tv' && (
                <div className="episode-group-picker">
                  <p>
                    {t('database.groups.currentOrder', {
                      order: subject.episode_group
                        ? t('database.groups.summary', {
                            name: subject.episode_group.name,
                            groups: subject.episode_group.group_count,
                            episodes: subject.episode_group.episode_count,
                          })
                        : t('database.groups.defaultOrder'),
                    })}
                  </p>
                  <Button
                    variant="secondary"
                    disabled={busy || !!importedTitle}
                    onClick={() =>
                      void run(t('database.groups.load'), async () => {
                        setGroups(
                          await call<EpisodeGroup[]>('list_episode_groups', {
                            id: subject.id,
                            refresh: false,
                          }),
                        )
                        setGroupsLoaded(true)
                      })
                    }
                  >
                    {t('database.groups.loadOptions')}
                  </Button>
                  {groupsLoaded && (
                    <Field label={t('database.groups.label')}>
                      <SelectField
                        aria-label={t('database.groups.label')}
                        value={subject.episode_group?.id || ''}
                        disabled={busy || !!importedTitle}
                        onValueChange={(id) =>
                          void load(subject.id, 'tmdb', 'tv', false, id)
                        }
                      >
                        <SelectOption value="">
                          {t('database.groups.defaultSeasons')}
                        </SelectOption>
                        {groups.map((g) => (
                          <SelectOption key={g.id} value={g.id}>
                            {t(
                              'database.groups.option',
                              {
                                name: g.name,
                                type:
                                  [
                                    '',
                                    t('database.groups.order.airDate'),
                                    t('database.groups.order.absolute'),
                                    'DVD',
                                    t('database.groups.order.digital'),
                                    t('database.groups.order.storyline'),
                                    t('database.groups.order.production'),
                                    t('database.groups.order.broadcast'),
                                  ][g.type] || t('common.media.other'),
                                groups: g.group_count,
                                episodes: g.episode_count,
                                id: g.id,
                              },
                            )}
                          </SelectOption>
                        ))}
                      </SelectField>
                    </Field>
                  )}
                  {subject.episode_group && (
                    <p className="muted">
                      {t(
                        'database.groups.orderHint',
                        { description: subject.episode_group.description },
                      )}
                    </p>
                  )}
                  <Disclosure>
                    <DisclosureTrigger>{t('database.groups.preview')}</DisclosureTrigger>
                    {(subject.seasons || []).map((s) => (
                      <p key={s.number}>
                        {t('database.season.summary', {
                          season: s.number,
                          title: s.title,
                          count: subject.episodes.filter(
                            (e) => e.season === s.number,
                          ).length,
                        })}
                      </p>
                    ))}
                  </Disclosure>
                </div>
              )}
              {subject.overview && subject.provider !== 'musicbrainz' && (
                <Disclosure>
                  <DisclosureTrigger>{t('common.fields.overview')}</DisclosureTrigger>
                  <p className="overview">{subject.overview}</p>
                </Disclosure>
              )}
              {importedTitle ? (
                <Alert role="status">
                  <AlertDescription>
                    {importedTitle.updated
                      ? t('database.import.updated', {
                          title: importedTitle.title,
                        })
                      : t('database.import.added', {
                          title: importedTitle.title,
                        })}
                  </AlertDescription>
                </Alert>
              ) : (
                <>
                  <div className="inline-fields">
                    <Field label={t('database.import.kind')}>
                      <SelectField
                        value={targetKind}
                        disabled={matches.length > 0}
                        onValueChange={(value) => {
                          setTargetKind(value)
                          setExisting('')
                        }}
                      >
                        <SelectOption
                          value="tv"
                          disabled={subject.kind === 'music'}
                        >
                          {t('common.media.series')}
                        </SelectOption>
                        <SelectOption
                          value="movie"
                          disabled={subject.kind === 'music'}
                        >
                          {t('database.kind.movie')}
                        </SelectOption>
                        <SelectOption
                          value="music"
                          disabled={subject.kind !== 'music'}
                        >
                          {t('database.kind.album')}
                        </SelectOption>
                      </SelectField>
                    </Field>
                    <Field label={t('database.import.link')}>
                      <SelectField
                        value={existing}
                        disabled={matches.length === 1}
                        onValueChange={(value) => {
                          const id = value
                          setExisting(id)
                          setGroupConfirmed(false)
                          const w = works.find((w) => w.id === id)
                          if (w) {
                            setTitle(w.title)
                            setTargetKind(w.kind)
                            setSeason(
                              importAssociation(works, subject, id).season,
                            )
                            setTmdb(w.tmdb_id)
                            setParent(w.parent_id)
                          }
                        }}
                      >
                        {!matches.length && (
                          <SelectOption value="">{t('database.import.create')}</SelectOption>
                        )}
                        {works
                          .filter((w) =>
                            matches.length
                              ? matches.some((match) => match.id === w.id)
                              : w.kind === targetKind,
                          )
                          .map((w) => (
                            <SelectOption key={w.id} value={w.id}>
                              {w.title}
                            </SelectOption>
                          ))}
                      </SelectField>
                    </Field>
                  </div>
                  {matches.length > 1 && (
                    <p className="muted">
                      {t('database.import.duplicateId')}
                    </p>
                  )}
                  <Field
                    label={
                      targetKind === 'music'
                        ? t('database.import.albumDirectory')
                        : t('database.import.workDirectory')
                    }
                  >
                    <Input
                      value={title}
                      disabled={Boolean(existing)}
                      onChange={(e) => setTitle(e.target.value)}
                    />
                  </Field>
                  {targetKind === 'tv' && subject.provider !== 'tmdb' && (
                    <Field label={t('database.import.localSeason')}>
                      <Input
                        type="number"
                        min="0"
                        max="999"
                        value={season}
                        disabled={!!binding}
                        onChange={(e) => setSeason(Number(e.target.value))}
                      />
                    </Field>
                  )}
                  {subject.provider === 'tmdb' && targetKind === 'tv' && (
                    <p className="muted">
                      {subject.episode_group
                        ? t('database.import.groupSeasons')
                        : t('database.import.tmdbSeasons')}
                    </p>
                  )}
                  {!existing && targetKind !== 'music' && (
                    <Field
                      label={t('database.import.tmdbIdHint')}
                    >
                      <Input
                        inputMode="numeric"
                        value={tmdb}
                        onChange={(e) => setTmdb(e.target.value)}
                        placeholder={t('database.import.unlinkedHint')}
                      />
                    </Field>
                  )}
                  {targetKind === 'movie' && (
                    <Field label={t('database.import.seriesHint')}>
                      <SelectField
                        value={parent}
                        onValueChange={(value) => setParent(value)}
                      >
                        <SelectOption value="">{t('database.import.standaloneMovie')}</SelectOption>
                        {works
                          .filter((w) => w.kind === 'tv')
                          .map((w) => (
                            <SelectOption key={w.id} value={w.id}>
                              {w.title}
                            </SelectOption>
                          ))}
                      </SelectField>
                    </Field>
                  )}
                  {groupChanged && (
                    <div className="metadata-errors">
                      <p>
                        {t(
                          'database.groups.changeSummary',
                          { renumbered: moved.length, retained: retained.length },
                        )}
                      </p>
                      <Disclosure>
                        <DisclosureTrigger>
                          {t('database.groups.reviewChanges')}
                        </DisclosureTrigger>
                        {moved.map((e) => {
                          const old = previousEpisodes.find(
                            (p) => p.remote_id === e.id,
                          )!
                          return (
                            <p key={e.id}>
                              {e.title} · S{old.season}E{old.number} → S
                              {e.season}E{e.sort}
                            </p>
                          )
                        })}
                        {retained.map((e) => (
                          <p key={e.id}>
                            {t('database.groups.disabledEpisode', {
                              title: e.title,
                              season: e.season,
                              episode: e.number,
                            })}
                          </p>
                        ))}
                      </Disclosure>
                      <Label className="check-label">
                        <Checkbox
                          checked={groupConfirmed}
                          onCheckedChange={(checked) =>
                            setGroupConfirmed(checked === true)
                          }
                        />
                        {t('database.groups.confirmChanges')}
                      </Label>
                    </div>
                  )}
                  {mutationBusy && (
                    <p className="muted">
                      {t(
                        'database.import.taskRunning',
                      )}
                    </p>
                  )}
                  <Button
                    className="full-width"
                    disabled={
                      busy ||
                      mutationBusy ||
                      (matches.length > 0 && !updating) ||
                      !title.trim() ||
                      (groupChanged && !groupConfirmed)
                    }
                    onClick={() => {
                      if (importing.current || busy || mutationBusy) return
                      importing.current = true
                      void run(t('database.import.saveBinding'), async () => {
                        setRemoteError('')
                        try {
                          const w = await call<Work>('bind_subject', {
                            request: {
                              work_id: existing || null,
                              subject,
                              group_change_confirmed: groupConfirmed,
                              title,
                              kind: targetKind,
                              season,
                              tmdb_id: tmdb,
                              parent_id: parent,
                            },
                          })
                          setImported((previous) => ({
                            ...previous,
                            [subjectKey(subject)]: {
                              title: w.title,
                              updated: updating,
                            },
                          }))
                          await onBound(w)
                        } catch (e) {
                          setRemoteError(String(e))
                          throw e
                        } finally {
                          importing.current = false
                        }
                      })
                    }}
                  >
                    <Link2Icon />
                    {groupChanged
                      ? t('database.groups.confirmSwitch')
                      : updating
                        ? t('database.import.update')
                        : existing
                          ? targetKind === 'music'
                            ? t('database.import.confirmAlbum')
                            : t('database.import.confirmSeasons')
                          : t('database.import.add')}
                  </Button>
                </>
              )}
              {subject.relations.length > 0 && (
                <Disclosure>
                  <DisclosureTrigger>
                    {t('library.bindings.item', {
                      count: subject.relations.length,
                    })}
                  </DisclosureTrigger>
                  {subject.relations.map((r) => (
                    <Button
                      className="relation"
                      key={`${r.id}-${r.relation}`}
                      disabled={busy}
                      onClick={() => void load(r.id, 'bangumi', 'tv')}
                      variant="ghost"
                    >
                      <span>{r.relation}</span>
                      {r.title} <small>#{r.id}</small>
                    </Button>
                  ))}
                </Disclosure>
              )}
            </div>
          ) : (
            <Empty title={t('database.details.empty')}>
              {t('database.dialog.title')}
            </Empty>
          )}
        </section>
      </div>
    </div>
  )
}
