import { CheckIcon, PlusIcon, ReloadIcon } from '@radix-ui/react-icons'
import { convertFileSrc } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useEffect, useRef, useState } from 'react'
import { Disclosure, DisclosureTrigger } from '@/components/disclosure'
import { Empty } from '@/components/empty-state'
import { Field } from '@/components/field'
import { FilePath } from '@/components/file-path'
import { Pages } from '@/components/pagination'
import { SelectField, SelectOption } from '@/components/select-field'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import { call } from '@/lib/api'
import { artwork } from '@/lib/artwork'
import { bytes } from '@/lib/format'
import { episodeKind, seasonEpisode } from '@/lib/media'
import type {
  Episode,
  LibraryIndex,
  Plan,
  ReplacementTarget,
  Run,
  Work,
} from '@/types'
import { catalogSeasons, seasonKey, seasonLabel } from './catalog'
import DeleteWorkButton from './DeleteWorkButton'
import ImportPreview from './ImportPreview'
import ExtraFiles from './ExtraFiles'
import type { LibraryDrop, SubmitMode } from './import-model'
import { useLibraryImport } from './use-library-import'

interface Props {
  onReplace: (target: ReplacementTarget) => void
  onRename: (paths: string[]) => void
  onBack: () => void
  onAdd: (target: {
    kind: string
    workId?: string
    episodeGroups?: boolean
  }) => void
  works: Work[]
  plans: Plan[]
  onDelete: (id: string) => Promise<void>
  workId: string
  onWork: (id: string) => void
  index: LibraryIndex
  busy: boolean
  importBusy?: boolean
  run: Run
  onReload: () => Promise<void>
  onPlan: (p: Plan, mode: SubmitMode) => Promise<void>
  dropRequest: LibraryDrop | null
  onDropHandled: () => void
}
export default function WorkDetail({
  works,
  plans,
  onDelete,
  onBack,
  onAdd,
  workId,
  index,
  busy,
  importBusy,
  run,
  onReload,
  onPlan,
  dropRequest,
  onDropHandled,
  onReplace,
  onRename,
}: Props) {
  const work = works.find((w) => w.id === workId)
  const imports = useLibraryImport(workId, run, onPlan, work?.kind)
  const consumedDrop = useRef(0)
  const [draft, setDraft] = useState(work),
    [page, setPage] = useState(1),
    [section, setSection] = useState('info'),
    [onlyMissing, setOnlyMissing] = useState(false),
    [folderBindings, setFolderBindings] = useState<Record<string, string>>({})
  const previousWork = useRef(work)
  useEffect(() => {
    const previous = previousWork.current
    setDraft((current) =>
      current?.id !== work?.id || JSON.stringify(current) === JSON.stringify(previous)
        ? work : current,
    )
    previousWork.current = work
    setPage(1)
  }, [work])
  const seasons = draft ? catalogSeasons(draft, index.assets) : []
  const selectedSeason = seasons.find((s) => String(s.number) === section)
  const showingInfo = section === 'info'
  const showingExtras = section === 'extras'
  const showResources =
    !showingExtras && (draft?.kind === 'movie' || !showingInfo)
  useEffect(() => {
    setSection('info')
    setOnlyMissing(false)
  }, [workId])
  useEffect(() => {
    setPage(1)
  }, [section, onlyMissing])
  useEffect(() => {
    if (
      section !== 'info' &&
      section !== 'extras' &&
      !seasons.some((s) => String(s.number) === section)
    )
      setSection('info')
  }, [section, seasons])
  const remoteBinding = (id?: string) =>
    draft?.bindings.some(
      (b) =>
        ['tmdb', 'bangumi', 'musicbrainz'].includes(b.provider) &&
        (!id || b.id === id),
    )
  const dirty = JSON.stringify(work) !== JSON.stringify(draft)
  const addingBusy = importBusy ?? busy
  const previewOutdated = !!imports.preview &&
    JSON.stringify(imports.preview.works.find((item) => item.id === workId)) !== JSON.stringify(work)
  const saveAndRefresh = async () => {
    if (dirty) {
      await call('save_work', { work: draft })
      await onReload()
    }
    if (imports.preview) await imports.refresh()
  }
  useEffect(() => {
    if (
      !dropRequest ||
      dropRequest.workId !== workId ||
      consumedDrop.current === dropRequest.id
    )
      return
    consumedDrop.current = dropRequest.id
    onDropHandled()
    if (dropRequest.episodeId ? dirty || busy : addingBusy) return
    if (!dropRequest.episodeId) setSection('extras')
    void imports.stage(dropRequest.paths, dropRequest.episodeId)
  }, [dropRequest, workId, onDropHandled, dirty, busy, addingBusy, imports])
  const available = (ep: Episode) =>
    index.assets.filter(
      (a) =>
        a.work_id === workId &&
        a.season === ep.season &&
        ep.number >= a.number &&
        ep.number <= a.end,
    )
  const episodes = (draft?.episodes || [])
    .filter(
      (e) =>
        (draft?.kind === 'movie' || e.season === Number(section)) &&
        (!onlyMissing || available(e).length === 0),
    )
    .sort(
      (a, b) => a.season - b.season || a.number - b.number || a.sort - b.sort,
    )
  const edit = (id: string, patch: Partial<Episode>) =>
    setDraft((d) =>
      d
        ? {
            ...d,
            episodes: d.episodes.map((ep) =>
              ep.id === id ? { ...ep, ...patch } : ep,
            ),
          }
        : d,
    )
  const assign = (ep: Episode) =>
    run(t('library.import.chooseEpisodeVideo'), async () => {
      const path = await open({
        multiple: true,
        filters: [
          {
            name: t('common.files.media'),
            extensions: [
              'flac',
              'mp3',
              'm4a',
              'wav',
              'ogg',
              'ape',
              'opus',
              'mkv',
              'mp4',
              'avi',
              'm4v',
              'mov',
              'ts',
              'm2ts',
              'webm',
            ],
          },
        ],
      })
      if (path && draft)
        await imports.stage(Array.isArray(path) ? path : [path], ep.id)
    })
  const chooseExtras = (directory: boolean) =>
    run(t('library.extras.chooseFiles'), async () => {
      const paths = await open({ multiple: true, directory })
      if (paths) await imports.stage(Array.isArray(paths) ? paths : [paths])
    })
  if (!draft) return <Empty title={t('library.empty.noWorks')}>{t('library.actions.addWorkHint')}</Empty>
  const infoLabel =
    draft.kind === 'tv'
      ? t('library.details.series')
      : draft.kind === 'music'
        ? t('library.details.album')
        : t('library.details.movie')
  return (
    <section className="work-detail-layout">
      <aside className="work-detail-menu">
        <Button variant="ghost" className="work-back" onClick={onBack}>
          {t('library.details.back')}
        </Button>
        <div className="work-menu-title">{draft.title}</div>
        <nav aria-label={t('library.details.title')}>
          <Button
            variant="ghost"
            className="work-menu-item"
            aria-current={showingInfo ? 'page' : undefined}
            onClick={() => setSection('info')}
          >
            {infoLabel}
          </Button>
          {draft.kind !== 'movie' &&
            seasons.map((season) => (
              <Button
                key={season.number}
                variant="ghost"
                className="work-menu-item season-menu-item"
                data-season={season.number}
                aria-current={
                  section === String(season.number) ? 'page' : undefined
                }
                onClick={() => setSection(String(season.number))}
              >
                <span className="season-menu-line">
                  <strong>{seasonLabel(season.number, draft.kind)}</strong>
                  <Badge
                    className="season-completeness"
                    tone={
                      !index.available
                        ? 'amber'
                        : season.total > 0 && season.owned === season.total
                          ? 'green'
                          : 'gray'
                    }
                    title={t(
                      'library.details.coverage',
                      {
                        scanPrefix: index.available ? '' : t('library.details.lastScan'),
                        owned: season.owned,
                        total: season.total,
                      },
                    )}
                  >
                    {season.owned}/{season.total}
                  </Badge>
                </span>
                {season.title && (
                  <span className="season-menu-name">{season.title}</span>
                )}
              </Button>
            ))}
          <Button
            variant="ghost"
            className="work-menu-item"
            aria-current={showingExtras ? 'page' : undefined}
            data-drop-extras
            data-drop-work={workId}
            data-drop-disabled={addingBusy ? 'true' : 'false'}
            onClick={() => setSection('extras')}
          >
            {t('common.files.extra')}
          </Button>
        </nav>
        {draft.kind === 'tv' && (
          <Button
            variant="secondary"
            disabled={busy || dirty}
            onClick={() => onAdd({ kind: draft.kind, workId })}
          >
            <PlusIcon />
            {t('library.seasons.add')}
          </Button>
        )}
        {draft.kind === 'tv' &&
          draft.bindings.some((b) => b.provider === 'tmdb') && (
            <Button
              variant="secondary"
              disabled={(importBusy ?? busy) || dirty}
              onClick={() =>
                onAdd({ kind: draft.kind, workId, episodeGroups: true })
              }
            >
              {t('library.groups.switch')}
            </Button>
          )}
        <Button
          variant="outline"
          disabled={
            busy || dirty || !index.assets.some((a) => a.work_id === workId)
          }
          onClick={() =>
            onRename(
              index.assets
                .filter((a) => a.work_id === workId)
                .map((a) => a.path),
            )
          }
        >
          {t('library.actions.renameResources')}
        </Button>
        <DeleteWorkButton
          work={work || draft}
          index={index}
          plans={plans}
          disabled={busy || dirty || !!imports.preview}
          run={run}
          onDelete={onDelete}
        />
      </aside>
      <div className="work-detail-content">
        <div className="work-detail-toolbar">
          <h2>
            {showingExtras
              ? t('common.files.extra')
              : showingInfo
                ? infoLabel
                : seasonLabel(Number(section), draft.kind)}
          </h2>
          <div className="button-row">
            {dirty && (
              <span className="unsaved-label">{t('library.editor.unsaved')}</span>
            )}
            {showingInfo && (
              <Button
                variant="secondary"
                disabled={busy || dirty || !remoteBinding()}
                onClick={() =>
                  void run(t('library.actions.syncMetadata'), async () => {
                    await call('sync_work', {
                      workId,
                      bindingId: null,
                      season: null,
                    })
                    await onReload()
                  })
                }
              >
                <ReloadIcon />
                {t('library.actions.sync')}
              </Button>
            )}
            <Button
              disabled={busy || !dirty}
              onClick={() =>
                void run(t('library.actions.save'), async () => {
                  await saveAndRefresh()
                })
              }
            >
              <CheckIcon />
              {t('library.actions.save')}
            </Button>
          </div>
        </div>
        {showingInfo && (
          <>
            <div className="work-heading">
              {artwork(draft) && (
                <img className="work-poster" src={artwork(draft)} alt="" />
              )}
              <div>
                <Badge tone="gray">
                  {draft.kind === 'movie'
                    ? t('database.kind.movie')
                    : draft.kind === 'music'
                      ? t('database.kind.album')
                      : t('common.media.tv')}
                </Badge>
                <h2>{draft.title}</h2>
                <p>
                  {draft.info?.localized_title !== draft.title &&
                    draft.info?.localized_title}
                </p>
                <p className="muted">
                  {draft.info?.original_title} · {draft.year} ·{' '}
                  {draft.info?.language || '—'} /{' '}
                  {draft.info?.original_language || '—'}
                </p>
                <p className="muted">
                  {t('library.details.bindingSummary', {
                    sources: draft.bindings.length,
                    chapters: draft.episodes.filter((e) => e.enabled).length,
                  })}
                </p>
              </div>
            </div>
            {draft.overview && (
              <p className="detail-overview">{draft.overview}</p>
            )}
            {!!draft.metadata_errors?.length && (
              <Disclosure className="metadata-errors">
                <DisclosureTrigger>
                  {t('library.details.incompleteMetadata')}
                </DisclosureTrigger>
                {draft.metadata_errors.map((e, i) => (
                  <p key={i}>{backendMessage(e)}</p>
                ))}
              </Disclosure>
            )}
            {!!draft.metadata_notices?.length && (
              <Disclosure>
                <DisclosureTrigger>
                  {t('tasks.metadata.noticeCount', {
                    count: draft.metadata_notices.length,
                  })}
                </DisclosureTrigger>
                {draft.metadata_notices.map((notice, i) => (
                  <p className="path-text" key={i}>
                    {backendMessage(notice)}
                  </p>
                ))}
              </Disclosure>
            )}
            <Disclosure className="work-details">
              <DisclosureTrigger>
                {t('library.editor.title')}
              </DisclosureTrigger>
              <div className="inline-fields">
                <Field label={t('library.editor.displayTitle')}>
                  <Input
                    value={draft.title}
                    onChange={(e) =>
                      setDraft({ ...draft, title: e.target.value })
                    }
                  />
                </Field>
                <Field label={t('library.editor.category')}>
                  <Input
                    placeholder={t('library.editor.autoCategory')}
                    value={draft.category || ''}
                    onChange={(e) =>
                      setDraft({ ...draft, category: e.target.value })
                    }
                  />
                </Field>
                {draft.kind === 'music' && (
                  <Field label={t('music.fields.artist')}>
                    <Input
                      value={draft.info?.artist || ''}
                      onChange={(e) =>
                        setDraft({
                          ...draft,
                          info: { ...draft.info!, artist: e.target.value },
                        })
                      }
                    />
                  </Field>
                )}
                <Field label={t('common.fields.year')}>
                  <Input
                    value={draft.year}
                    onChange={(e) =>
                      setDraft({ ...draft, year: e.target.value })
                    }
                  />
                </Field>
                <Field label={t('library.editor.tmdbId')}>
                  <Input
                    value={draft.tmdb_id}
                    onChange={(e) =>
                      setDraft({ ...draft, tmdb_id: e.target.value })
                    }
                  />
                </Field>
              </div>
              {draft.kind === 'movie' && (
                <Field label={t('library.editor.series')}>
                  <SelectField
                    value={draft.parent_id}
                    onValueChange={(value) =>
                      setDraft({ ...draft, parent_id: value })
                    }
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
              {draft.bindings
                .filter((b) => b.provider === 'bangumi' && draft.kind === 'tv')
                .map((binding) => (
                  <div className="binding-row" key={binding.id}>
                    <div>
                      <strong>{binding.source_title}</strong>
                      <small>
                        {binding.provider} #{binding.remote_id}
                      </small>
                    </div>
                    <Label>
                      {t('library.editor.primarySeason')}
                      <Input
                        aria-label={t('library.editor.bindingSeason', {
                          title: binding.source_title,
                        })}
                        type="number"
                        min="0"
                        max="999"
                        className="number-input"
                        value={binding.season}
                        onChange={(e) => {
                          const season = Number(e.target.value)
                          setDraft({
                            ...draft,
                            bindings: draft.bindings.map((b) =>
                              b.id === binding.id ? { ...b, season } : b,
                            ),
                            seasons: draft.seasons?.map((s) =>
                              s.binding_id === binding.id &&
                              s.number === binding.season
                                ? { ...s, number: season }
                                : s,
                            ),
                            episodes: draft.episodes.map((ep) =>
                              ep.binding_id === binding.id && ep.kind === 0
                                ? { ...ep, season }
                                : ep,
                            ),
                          })
                        }}
                      />
                    </Label>
                  </div>
                ))}
              <p className="muted">
                {t('library.editor.bindingHint')}
              </p>
            </Disclosure>
          </>
        )}
        {!showingInfo && selectedSeason && (
          <section className="season-detail" aria-label={t('library.seasons.details')}>
            {!index.available && (
              <p className="muted">
                {t('library.details.offlineHint')}
              </p>
            )}
            {selectedSeason.seasons.map((season) => {
              const binding = draft.bindings.find(
                (b) => b.id === season.binding_id,
              )
              const poster = season.info?.local_poster
                ? convertFileSrc(season.info.local_poster)
                : season.info?.poster
              return (
                <div className="season-metadata" key={seasonKey(season)}>
                  {poster && (
                    <img
                      className="season-poster"
                      src={poster}
                      alt={t('library.seasons.poster')}
                    />
                  )}
                  <div className="season-metadata-fields">
                    <div className="season-metadata-heading">
                      <Field
                        label={
                          draft.kind === 'music' ? t('library.seasons.discName') : t('library.seasons.seasonName')
                        }
                      >
                        <Input
                          aria-label={t('library.seasons.nameLabel', {
                            season: season.number,
                            source:
                              selectedSeason.seasons.length > 1
                                ? ` · ${binding?.provider} #${binding?.remote_id}`
                                : '',
                          })}
                          value={season.title}
                          onChange={(e) =>
                            setDraft({
                              ...draft,
                              seasons: draft.seasons?.map((item) =>
                                seasonKey(item) === seasonKey(season)
                                  ? {
                                      ...item,
                                      title: e.target.value,
                                      custom_title: true,
                                    }
                                  : item,
                              ),
                            })
                          }
                        />
                      </Field>
                      <Button
                        variant="secondary"
                        disabled={
                          busy || dirty || !remoteBinding(season.binding_id)
                        }
                        onClick={() =>
                          void run(t('library.seasons.syncAction'), async () => {
                            await call('sync_work', {
                              workId,
                              bindingId: season.binding_id,
                              season: season.remote_number,
                            })
                            await onReload()
                          })
                        }
                      >
                        <ReloadIcon />
                        {draft.kind === 'music' ? t('library.seasons.syncDisc') : t('library.seasons.syncSeason')}
                      </Button>
                    </div>
                    {season.info?.original_title &&
                      season.info.original_title !== season.title && (
                        <p className="muted">
                          {t('library.seasons.originalName', {
                            title: season.info.original_title,
                          })}
                        </p>
                      )}
                    <p className="season-source muted">
                      {binding?.provider} #{binding?.remote_id}
                      {season.info?.air_date && ` · ${season.info.air_date}`}
                    </p>
                    {season.info?.overview && (
                      <p className="detail-overview">{season.info.overview}</p>
                    )}
                  </div>
                </div>
              )
            })}
            {!selectedSeason.seasons.length && (
              <p className="muted">
                {selectedSeason.title ||
                  (Number(section) === 0 ? t('common.media.special') : t('library.seasons.empty'))}
              </p>
            )}
          </section>
        )}
        {showingExtras && (
          <section
            className="library-extra-drop"
            aria-label={t('library.extras.dropTitle')}
            data-drop-extras
            data-drop-work={workId}
            data-drop-disabled={addingBusy ? 'true' : 'false'}
          >
            <h3>{t('library.extras.dropPlaceholder')}</h3>
            <p className="muted">
              {t(
                'library.extras.description',
              )}
            </p>
            <div className="button-row">
              <Button
                variant="secondary"
                disabled={addingBusy}
                onClick={() => void chooseExtras(false)}
              >
                {t('common.actions.chooseFile')}
              </Button>
              <Button
                variant="outline"
                disabled={addingBusy}
                onClick={() => void chooseExtras(true)}
              >
                {t('common.actions.chooseDirectory')}
              </Button>
            </div>
          </section>
        )}
        {showingExtras && <ExtraFiles workId={workId} revision={index.scanned_at} />}
        <ImportPreview
          draft={imports}
          disabled={addingBusy}
          needsSave={dirty}
          needsRefresh={previewOutdated}
          onSave={() => run(t('library.actions.save'), saveAndRefresh)}
        />
        {showResources && (
          <section
            className="season-resources"
            aria-label={t('library.resources.title')}
          >
            <div className="selection-bar">
              <h3>
                {draft.kind === 'movie'
                  ? t('library.episodes.movieResources')
                  : draft.kind === 'music'
                    ? t('library.episodes.musicResources')
                    : t('library.episodes.tvResources')}
              </h3>
              <span className="muted">
                {selectedSeason
                  ? t('library.episodes.coverage', {
                      owned: selectedSeason.owned,
                      total: selectedSeason.total,
                    })
                  : t('library.episodes.chapterCount', { count: episodes.length })}
              </span>
              <Button
                size="sm"
                variant="outline"
                disabled={
                  busy ||
                  dirty ||
                  !index.assets.some(
                    (a) =>
                      a.work_id === workId &&
                      (draft.kind === 'movie' || a.season === Number(section)),
                  )
                }
                onClick={() =>
                  onReplace({
                    paths: index.assets
                      .filter(
                        (a) =>
                          a.work_id === workId &&
                          (draft.kind === 'movie' ||
                            a.season === Number(section)),
                      )
                      .map((a) => a.path),
                    manual: false,
                  })
                }
              >
                {t('resources.replace.scan')}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={
                  busy ||
                  dirty ||
                  !index.assets.some(
                    (a) =>
                      a.work_id === workId &&
                      (draft.kind === 'movie' || a.season === Number(section)),
                  )
                }
                onClick={() =>
                  onRename(
                    index.assets
                      .filter(
                        (a) =>
                          a.work_id === workId &&
                          (draft.kind === 'movie' ||
                            a.season === Number(section)),
                      )
                      .map((a) => a.path),
                  )
                }
              >
                {draft.kind === 'movie' ? t('library.actions.renameMovie') : t('library.actions.renameSeason')}
              </Button>
              <Label className="check-label">
                <Checkbox
                  checked={onlyMissing}
                  onCheckedChange={(v) => setOnlyMissing(v === true)}
                />
                {t('library.episodes.onlyMissing')}
              </Label>
            </div>
            <div className="table-wrap">
              <Table className="episode-table">
                <TableHeader>
                  <TableRow>
                    <TableHead>{t('library.episodes.columns.enabled')}</TableHead>
                    <TableHead>{t('library.episodes.columns.remote')}</TableHead>
                    <TableHead>{t('common.fields.title')}</TableHead>
                    <TableHead>
                      {draft.kind === 'music'
                        ? t('library.episodes.columns.track')
                        : t('library.episodes.columns.episode')}
                    </TableHead>
                    <TableHead>{t('library.episodes.columns.resources')}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {episodes.slice((page - 1) * 80, page * 80).map((ep) => {
                    const assets = available(ep)
                    const binding = draft.bindings.find(
                      (b) => b.id === ep.binding_id,
                    )
                    const titleDetails = [
                      ep.info?.original_title !== ep.title &&
                        ep.info?.original_title,
                      binding?.source_title,
                    ]
                      .filter(Boolean)
                      .join(' · ')
                    return (
                      <TableRow
                        key={ep.id}
                        data-drop-episode={ep.id}
                        data-drop-work={draft.id}
                        data-drop-disabled={
                          !ep.enabled ||
                          JSON.stringify(work) !== JSON.stringify(draft)
                            ? 'true'
                            : 'false'
                        }
                        className={ep.enabled ? '' : 'pending-episode'}
                      >
                        <TableCell>
                          <Checkbox
                            aria-label={t('library.episodes.enabledLabel', {
                              title: ep.title,
                            })}
                            checked={ep.enabled}
                            onCheckedChange={(v) =>
                              edit(ep.id, { enabled: v === true })
                            }
                          />
                        </TableCell>
                        <TableCell>
                          <div className="episode-chapter">
                            <strong>{ep.sort}</strong>
                            <span>{episodeKind(ep.kind)}</span>
                          </div>
                          <small className="episode-source">
                            {binding?.provider} #{ep.remote_id}
                          </small>
                        </TableCell>
                        <TableCell>
                          <Input
                            aria-label={t('library.episodes.titleLabel', {
                              number: ep.sort,
                            })}
                            value={ep.title}
                            onChange={(e) =>
                              edit(ep.id, { title: e.target.value })
                            }
                          />
                          {titleDetails && (
                            <small
                              className="episode-title-details"
                              title={titleDetails}
                            >
                              {titleDetails}
                            </small>
                          )}
                        </TableCell>
                        <TableCell>
                          <div className="number-pair">
                            <Input
                              aria-label={t('library.editor.bindingSeason', {
                                title: ep.title,
                              })}
                              type="number"
                              min="0"
                              max="999"
                              value={ep.season}
                              onChange={(e) =>
                                edit(ep.id, { season: Number(e.target.value) })
                              }
                            />
                            <span>/</span>
                            <Input
                              aria-label={t('library.episodes.numberLabel', {
                                title: ep.title,
                              })}
                              type="number"
                              min="1"
                              max="9999"
                              value={ep.number}
                              onChange={(e) =>
                                edit(ep.id, { number: Number(e.target.value) })
                              }
                            />
                          </div>
                          <small>{seasonEpisode(ep)}</small>
                        </TableCell>
                        <TableCell>
                          <div className="episode-resource-heading">
                            {imports.preview?.rows.some(
                              (r) =>
                                !r.parent_id &&
                                r.episode_ids.includes(ep.id) &&
                                r.action !== 'skip',
                            ) && <Badge tone="amber">{t('common.status.pendingSubmission')}</Badge>}
                            {assets.length ? (
                              <Badge tone="green">
                                {t(
                                  assets.some((a) => a.part)
                                    ? 'library.episodes.importedFileCount'
                                    : 'library.episodes.ownedCount',
                                  {
                                    count: assets.length,
                                  },
                                )}
                              </Badge>
                            ) : (
                              <Badge tone={index.available ? 'gray' : 'amber'}>
                                {index.available
                                  ? t('library.episodes.missing')
                                  : t('library.episodes.unknown')}
                              </Badge>
                            )}
                            <Button
                              size="xs"
                              variant="ghost"
                              disabled={
                                busy ||
                                !ep.enabled ||
                                JSON.stringify(work) !== JSON.stringify(draft)
                              }
                              onClick={() => void assign(ep)}
                            >
                              <PlusIcon />
                              {t('common.actions.addFile')}
                            </Button>
                          </div>
                          {assets.map((a) => (
                            <div className="episode-asset" key={a.path}>
                              {!!a.part && (
                                <Badge tone="gray">Part {a.part}</Badge>
                              )}
                              <FilePath path={a.path} truncate />
                              <div className="episode-asset-actions">
                                <span>{bytes(a.signature.size)}</span>
                                <Button
                                  size="xs"
                                  variant="ghost"
                                  disabled={busy || dirty}
                                  onClick={() =>
                                    onReplace({
                                      paths: [a.path],
                                      manual: true,
                                    })
                                  }
                                >
                                  {t('resources.replace.chooseFile')}
                                </Button>
                              </div>
                            </div>
                          ))}
                        </TableCell>
                      </TableRow>
                    )
                  })}
                </TableBody>
              </Table>
            </div>
            <Pages page={page} count={episodes.length} onChange={setPage} />
            {!episodes.length && (
              <Empty title={onlyMissing ? t('library.episodes.complete') : t('library.episodes.empty')}>
                {onlyMissing
                  ? t('library.episodes.emptyFilterHint')
                  : t('library.episodes.emptyHint')}
              </Empty>
            )}
          </section>
        )}
        {showingInfo && index.unbound.length > 0 && (
          <Disclosure className="unbound">
            <DisclosureTrigger>
              {t('library.unbound.discoveredCount', {
                count: index.unbound.length,
              })}
            </DisclosureTrigger>
            {index.unbound.map((folder) => (
              <div className="unbound-row" key={folder.path}>
                <div>
                  <strong>{folder.name}</strong>
                  <small className="path-text">
                    <FilePath path={folder.path} />
                    {t('library.unbound.videoCount', { count: folder.files })}
                  </small>
                </div>
                <SelectField
                  aria-label={t('library.unbound.workLabel', { directory: folder.name })}
                  value={folderBindings[folder.path] || ''}
                  onValueChange={(value) =>
                    setFolderBindings({
                      ...folderBindings,
                      [folder.path]: value,
                    })
                  }
                >
                  <SelectOption value="">{t('library.unbound.chooseWork')}</SelectOption>
                  {works
                    .filter((w) => w.kind === folder.kind)
                    .map((w) => (
                      <SelectOption key={w.id} value={w.id}>
                        {w.title}
                      </SelectOption>
                    ))}
                </SelectField>
                <Button
                  disabled={busy || !folderBindings[folder.path]}
                  size="sm"
                  onClick={() =>
                    void run(t('library.unbound.bind'), async () => {
                      await call('bind_library_folder', {
                        path: folder.path,
                        workId: folderBindings[folder.path],
                      })
                      await call('refresh_library')
                      await onReload()
                    })
                  }
                >
                  {t('common.actions.confirmLink')}
                </Button>
              </div>
            ))}
          </Disclosure>
        )}
      </div>
    </section>
  )
}
