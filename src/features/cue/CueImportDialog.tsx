import { Cross2Icon } from '@radix-ui/react-icons'
import { open } from '@tauri-apps/plugin-dialog'
import { useEffect, useState } from 'react'
import { ActionSplitButton } from '@/components/action-split-button'
import { Field } from '@/components/field'
import { FilePath } from '@/components/file-path'
import { SelectField, SelectOption } from '@/components/select-field'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/components/ui/dialog'
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
import { useRememberedAction } from '@/hooks/use-remembered-action'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import { call } from '@/lib/api'
import type { Plan, RemoteSubject, Run, Work } from '@/types'
import { cueTime } from './time'
import { cueChoices } from './matching'
import type { CueAnalysis, CueCandidates, CueSheet, TrackChoice } from './types'
import './cue.css'

export default function CueImportDialog({
  path,
  selectedPaths = [],
  defaultSourceDisposition = 'keep',
  works,
  run,
  onClose,
  onPlan,
}: {
  path: string
  selectedPaths?: string[]
  defaultSourceDisposition?: 'keep' | 'delete' | 'backup'
  works: Work[]
  run: Run
  onClose: () => void
  onPlan: (plan: Plan, mode: 'view' | 'execute') => Promise<void>
}) {
  const [encoding, setEncoding] = useState('')
  const [files, setFiles] = useState<Record<string, string>>({})
  const [sheet, setSheet] = useState<CueSheet>()
  const [readError, setReadError] = useState('')
  const [reading, setReading] = useState(true)
  const selectedKey = JSON.stringify(selectedPaths)
  const [analysis, setAnalysis] = useState<CueAnalysis>()
  const [candidates, setCandidates] = useState<CueCandidates>()
  const [query, setQuery] = useState('')
  const [release, setRelease] = useState<RemoteSubject>()
  const [local, setLocal] = useState(false)
  const [workId, setWorkId] = useState('')
  const [disc, setDisc] = useState(1)
  const [title, setTitle] = useState('')
  const [artist, setArtist] = useState('')
  const [date, setDate] = useState('')
  const [tracks, setTracks] = useState<TrackChoice[]>([])
  const [plan, setPlan] = useState<Plan>()
  const [busy, setBusy] = useState(false)
  const [analyzing, setAnalyzing] = useState(false)
  const step = plan ? 'organize' : analysis ? 'album' : 'parse'
  const [mode, setMode] = useRememberedAction<'view' | 'execute'>(
    'bangumi.cueCreateMode',
    ['view', 'execute'],
    'view',
  )
  const [submitted, setSubmitted] = useState(false)
  const [sourceDisposition, setSourceDisposition] = useState<string>(defaultSourceDisposition)
  const [sourceDispositionConfirmed, setSourceDispositionConfirmed] =
    useState(false)
  const perform = async (label: string, action: () => Promise<void>) => {
    if (busy) return
    setBusy(true)
    try {
      await run(label, action)
    } finally {
      setBusy(false)
    }
  }
  const reset = () => {
    setSourceDisposition(defaultSourceDisposition)
    setSourceDispositionConfirmed(false)
    setWorkId('')
    setAnalysis(undefined)
    setCandidates(undefined)
    setRelease(undefined)
    setLocal(false)
    setPlan(undefined)
    setTracks([])
  }
  const mapTracks = (a: CueAnalysis, s?: RemoteSubject, d = disc) => {
    setTracks(cueChoices(a, s, d))
    setPlan(undefined)
  }
  useEffect(() => {
    let active = true
    setReading(true)
    setReadError('')
    void call<CueSheet>('inspect_cue', {
      request: {
        path,
        encoding,
        files,
        selected_paths: JSON.parse(selectedKey),
      },
    })
      .then((s) => {
        if (active) setSheet(s)
      })
      .catch((error) => {
        if (active) setReadError(backendMessage(String(error)))
      })
      .finally(() => {
        if (active) setReading(false)
      })
    return () => {
      active = false
    }
  }, [path, encoding, files, selectedKey])
  const loadRelease = async (id: string) => {
    if (!analysis) return
    const s = await call<RemoteSubject>('media_detail', {
      provider: 'musicbrainz',
      id,
      kind: 'music',
      refresh: false,
      episodeGroupId: null,
    })
    setRelease(s)
    setLocal(false)
    setWorkId('')
    setTitle(s.title || analysis.sheet.title)
    setArtist(s.info?.artist || analysis.sheet.performer)
    setDate(s.info?.air_date || analysis.sheet.rem.DATE || '')
    const d = s.seasons?.[0]?.number || s.episodes[0]?.season || 1
    setDisc(d)
    mapTracks(analysis, s, d)
  }
  return (
    <Dialog
      open
      onOpenChange={(v) => {
        if (!v && !busy) onClose()
      }}
    >
      <DialogContent
        className="cue-dialog"
        showCloseButton={false}
        onInteractOutside={(e) => e.preventDefault()}
        onEscapeKeyDown={(e) => {
          if (busy) e.preventDefault()
        }}
      >
        <header className="cue-header">
          <div className="section-title">
            <DialogTitle>{t('cue.dialog.title')}</DialogTitle>
            <Button
              variant="ghost"
              size="icon-sm"
              disabled={busy}
              aria-label={t('common.actions.close')}
              onClick={onClose}
            >
              <Cross2Icon />
            </Button>
          </div>
          <DialogDescription>{t('cue.dialog.description')}</DialogDescription>
          <ol className="cue-steps" aria-label={t('cue.steps.label')}>
            {(['parse', 'album', 'organize'] as const).map((item, i) => (
              <li key={item} aria-current={step === item ? 'step' : undefined}>
                <span>{i + 1}</span>
                {item === 'parse'
                  ? t('cue.steps.parse')
                  : item === 'album'
                    ? t('cue.steps.album')
                    : t('cue.steps.organize')}
              </li>
            ))}
          </ol>
        </header>
        <div className="cue-columns">
          <main className="cue-review" aria-busy={busy}>
            {!analysis && sheet && (
              <>
                <h3>{sheet.title || t('music.fields.album')}</h3>
                <p className="muted">{t('cue.status.readyToAnalyze')}</p>
                {sheet.tracks && (
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>{t('cue.tracks.cueColumn')}</TableHead>
                        <TableHead>{t('music.fields.trackTitle')}</TableHead>
                        <TableHead>{t('cue.tracks.timeRange')}</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {sheet.tracks.map((track) => (
                        <TableRow key={track.number}>
                          <TableCell>{track.number}</TableCell>
                          <TableCell>{track.title || '—'}</TableCell>
                          <TableCell>
                            {track.index01 == null
                              ? '—'
                              : cueTime(track.index01 / 75)}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                )}
              </>
            )}
            {step === 'album' && analysis && (
              <>
                <p className="cue-format" role="status">
                  {release
                    ? t('cue.search.selected', { title: release.title, disc })
                    : local
                      ? t('cue.actions.useLocalAlbum')
                      : t('cue.search.chooseRelease')}
                </p>
                {local && (
                  <Field label={t('cue.album.local')}>
                    <SelectField
                      value={workId}
                      disabled={busy}
                      onValueChange={(id) => {
                        setWorkId(id)
                        setPlan(undefined)
                        const w = works.find((w) => w.id === id)
                        if (w) {
                          setTitle(w.title)
                          setArtist(w.info?.artist || '')
                          setDate(w.info?.air_date || '')
                        }
                      }}
                    >
                      <SelectOption value="">{t('cue.album.new')}</SelectOption>
                      {works
                        .filter((w) => w.kind === 'music')
                        .map((w) => (
                          <SelectOption key={w.id} value={w.id}>
                            {w.title}
                          </SelectOption>
                        ))}
                    </SelectField>
                  </Field>
                )}
                {(local || release) && (
                  <div className="cue-fields">
                    <Field label={t('music.fields.album')}>
                      <Input
                        value={title}
                        disabled={busy}
                        onChange={(e) => {
                          setTitle(e.target.value)
                          setPlan(undefined)
                        }}
                      />
                    </Field>
                    <Field label={t('music.fields.albumArtist')}>
                      <Input
                        value={artist}
                        disabled={busy}
                        onChange={(e) => {
                          setArtist(e.target.value)
                          setPlan(undefined)
                        }}
                      />
                    </Field>
                    <Field label={t('common.fields.date')}>
                      <Input
                        value={date}
                        disabled={busy}
                        onChange={(e) => {
                          setDate(e.target.value)
                          setPlan(undefined)
                        }}
                      />
                    </Field>
                    <Field label={t('music.fields.disc')}>
                      {release ? (
                        <SelectField
                          value={String(disc)}
                          disabled={busy}
                          onValueChange={(v) => {
                            setDisc(Number(v))
                            mapTracks(analysis, release, Number(v))
                          }}
                        >
                          {(release.seasons || []).map((s) => (
                            <SelectOption
                              key={s.number}
                              value={String(s.number)}
                            >
                              {s.title || String(s.number)}
                            </SelectOption>
                          ))}
                        </SelectField>
                      ) : (
                        <Input
                          type="number"
                          min={1}
                          max={999}
                          value={disc}
                          disabled={busy}
                          onChange={(e) => {
                            setDisc(Number(e.target.value))
                            setPlan(undefined)
                          }}
                        />
                      )}
                    </Field>
                  </div>
                )}
                {analysis.sources.map((s) => (
                  <p key={s.file.path} className="muted cue-format">
                    {s.codec.toUpperCase()} · {s.rate} Hz · {s.bits || '—'} bit
                    · {s.channels} ch
                  </p>
                ))}
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>{t('cue.tracks.cueColumn')}</TableHead>
                      <TableHead>
                        {release
                          ? t('cue.tracks.databaseColumn')
                          : local
                            ? t('cue.tracks.localColumn')
                            : t('cue.tracks.sourceColumn')}
                      </TableHead>
                      <TableHead>{t('cue.tracks.timeRange')}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {analysis.tracks.map((track, i) => (
                      <TableRow key={track.number}>
                        <TableCell>
                          {track.number}. {track.title || '—'}
                          <small className="cue-format">
                            {t('cue.tracks.actualDuration', {
                              time: cueTime(
                                track.cut
                                  ? track.cut.end - track.cut.start
                                  : track.duration,
                              ),
                            })}{' '}
                            · {track.extension.toUpperCase()}
                          </small>
                          {track.cut && (
                            <small className="cue-format">
                              {t('cue.tracks.cutOffset')}{' '}
                              {Math.round(track.cut.start_offset * 1000)} /{' '}
                              {Math.round(track.cut.end_offset * 1000)} ms
                            </small>
                          )}
                        </TableCell>
                        <TableCell>
                          {!local && !release ? (
                            <span className="muted">
                              {t('cue.search.chooseSource')}
                            </span>
                          ) : (
                            <>
                              {release && (
                                <SelectField
                                  aria-label={`${t('cue.tracks.databaseTrack')} ${track.number}`}
                                  disabled={busy}
                                  value={tracks[i]?.remote_id || ''}
                                  onValueChange={(id) => {
                                    const r = release.episodes.find(
                                      (e) => e.id === id,
                                    )
                                    if (!r) {
                                      setTracks(
                                        tracks.map((x, j) =>
                                          j === i ? { ...x, remote_id: '' } : x,
                                        ),
                                      )
                                      setPlan(undefined)
                                      return
                                    }
                                    setTracks(
                                      tracks.map((x, j) =>
                                        j === i
                                          ? {
                                              ...x,
                                              remote_id: id,
                                              title: r.title || track.title,
                                              performer:
                                                r.info?.artist ||
                                                track.performer,
                                            }
                                          : x,
                                      ),
                                    )
                                    setPlan(undefined)
                                  }}
                                >
                                  <SelectOption value="">
                                    {t('cue.tracks.select')}
                                  </SelectOption>
                                  {release.episodes
                                    .filter(
                                      (e) => e.season === disc && e.sort > 0,
                                    )
                                    .map((e) => (
                                      <SelectOption key={e.id} value={e.id}>
                                        {e.sort}. {e.title} ·{' '}
                                        {e.info?.duration_ms != null
                                          ? cueTime(e.info.duration_ms / 1000)
                                          : t('common.values.unknown')}
                                      </SelectOption>
                                    ))}
                                </SelectField>
                              )}
                              <Input
                                aria-label={`${t('music.fields.trackTitle')} ${track.number}`}
                                disabled={busy}
                                value={tracks[i]?.title || ''}
                                onChange={(e) => {
                                  setTracks(
                                    tracks.map((x, j) =>
                                      j === i
                                        ? { ...x, title: e.target.value }
                                        : x,
                                    ),
                                  )
                                  setPlan(undefined)
                                }}
                              />
                              <Input
                                aria-label={`${t('music.fields.performer')} ${track.number}`}
                                disabled={busy}
                                value={tracks[i]?.performer || ''}
                                onChange={(e) => {
                                  setTracks(
                                    tracks.map((x, j) =>
                                      j === i
                                        ? { ...x, performer: e.target.value }
                                        : x,
                                    ),
                                  )
                                  setPlan(undefined)
                                }}
                              />
                            </>
                          )}
                        </TableCell>
                        <TableCell className="cue-format">
                          {track.segments.map((s) => (
                            <div key={s.source}>
                              {analysis.sources.length > 1 && (
                                <span>
                                  {t('cue.sources.number', {
                                    number: s.source + 1,
                                  })}{' '}
                                  ·{' '}
                                </span>
                              )}
                              {cueTime(
                                s.start / analysis.sources[s.source].rate,
                              )}
                              –
                              {cueTime(s.end / analysis.sources[s.source].rate)}
                            </div>
                          ))}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </>
            )}
            {plan && (
              <div className="cue-outputs">
                <p className="muted cue-output-hint">{t('cue.outputs.hint')}</p>
                {plan.rows.map((row, i) => (
                  <div key={row.id} className="cue-output">
                    <div className="cue-output-heading">
                      <strong>
                        {tracks[i]?.number}. {tracks[i]?.title}
                      </strong>
                      <Badge variant="outline">
                        {row.target_signature
                          ? t('cue.outputs.existing')
                          : t('cue.outputs.planned')}
                      </Badge>
                    </div>
                    {analysis?.tracks[i] && (
                      <span className="cue-format">
                        {t('cue.tracks.actualDuration', {
                          time: cueTime(
                            analysis.tracks[i].cut
                              ? analysis.tracks[i].cut!.end -
                                  analysis.tracks[i].cut!.start
                              : analysis.tracks[i].duration,
                          ),
                        })}
                      </span>
                    )}
                    <FilePath
                      path={row.target}
                      openButton={!!row.target_signature}
                    />
                    <SelectField
                      aria-label={`${t('common.operations.label')} ${i + 1}`}
                      disabled={busy}
                      value={row.action}
                      onValueChange={(action) =>
                        setPlan({
                          ...plan,
                          rows: plan.rows.map((r, j) =>
                            j === i ? { ...r, action } : r,
                          ),
                        })
                      }
                    >
                      <SelectOption
                        value={row.target_signature ? 'replace' : 'copy'}
                      >
                        {row.target_signature
                          ? t('cue.conflict.replace')
                          : t('common.operations.cueSplit')}
                      </SelectOption>
                      <SelectOption value="skip">
                        {t('common.actions.skip')}
                      </SelectOption>
                    </SelectField>
                    {row.backups.map((b) => (
                      <div key={b.backup} className="cue-output-backup">
                        <span className="cue-format">
                          {t('cue.outputs.backup')}
                        </span>
                        <FilePath path={b.backup} openButton={false} />
                      </div>
                    ))}
                    {row.warnings.length > 0 && (
                      <Label>
                        <Checkbox
                          disabled={busy}
                          checked={row.reviewed}
                          onCheckedChange={(v) =>
                            setPlan({
                              ...plan,
                              rows: plan.rows.map((r, j) =>
                                j === i ? { ...r, reviewed: v === true } : r,
                              ),
                            })
                          }
                        />
                        {t('cue.review.warningsConfirmed')}
                        <span>
                          {row.warnings
                            .map((w) => backendMessage(w))
                            .join(' · ')}
                        </span>
                      </Label>
                    )}
                  </div>
                ))}
              </div>
            )}
          </main>
          <aside className="cue-sidebar">
            <FilePath path={path} />
            {step === 'parse' && (
              <>
                <Field label={t('cue.fields.encoding')}>
                  <SelectField
                    value={encoding}
                    disabled={busy}
                    onValueChange={(v) => {
                      setEncoding(v)
                      setSheet(undefined)
                      reset()
                    }}
                  >
                    <SelectOption value="">UTF-8 / BOM</SelectOption>
                    <SelectOption value="gb18030">GB18030</SelectOption>
                    <SelectOption value="shift-jis">Shift-JIS</SelectOption>
                  </SelectField>
                </Field>
                <p className="cue-format" role="status">
                  {reading
                    ? ''
                    : sheet
                      ? t('cue.status.read', {
                          count: sheet.tracks?.length || 0,
                        })
                      : ''}
                </p>
                {readError && <p role="alert">{readError}</p>}
                {readError && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => setFiles({ ...files })}
                  >
                    {t('cue.actions.read')}
                  </Button>
                )}
                {sheet?.files.map((name, index) => {
                  const reference = sheet.references?.find(
                    (r) => r.name === name,
                  )
                  const source = analysis?.sources[index]
                  const resolved = source?.file.path || reference?.path
                  return (
                    <div key={name} className="cue-reference">
                      <Badge variant={resolved ? 'secondary' : 'outline'}>
                        {resolved
                          ? t('cue.sources.matched')
                          : t('cue.sources.missing')}
                      </Badge>
                      <FilePath path={resolved || files[name] || name} />
                      {source && (
                        <span className="cue-format">
                          {source.codec.toUpperCase()} · {source.rate} Hz ·{' '}
                          {source.bits ? `${source.bits}-bit · ` : ''}
                          {t('cue.sources.channels', {
                            count: source.channels,
                          })}
                        </span>
                      )}
                      {reference?.error && !resolved && (
                        <span className="cue-format">
                          {backendMessage(reference.error)}
                        </span>
                      )}
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={busy}
                        onClick={() =>
                          void perform(
                            t('cue.actions.chooseAudio'),
                            async () => {
                              const chosen = await open({
                                multiple: false,
                                defaultPath: path.slice(0, Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')) + 1) || undefined,
                              })
                              if (typeof chosen === 'string') {
                                setFiles({ ...files, [name]: chosen })
                                reset()
                              }
                            },
                          )
                        }
                      >
                        {resolved
                          ? t('cue.actions.changeAudio')
                          : t('cue.actions.chooseAudio')}
                      </Button>
                    </div>
                  )
                })}
              </>
            )}
            {step === 'album' && analysis && (
              <>
                <p className="cue-format" role="status">
                  {candidates
                    ? candidates.items.length
                      ? t('cue.search.results', {
                          count: candidates.items.length,
                        })
                      : t('cue.search.empty')
                    : t('cue.search.pending')}
                </p>
                <Field label={t('cue.actions.searchAlbum')}>
                  <Input
                    value={query}
                    disabled={busy}
                    onChange={(e) => setQuery(e.target.value)}
                  />
                </Field>
                <Button
                  variant="outline"
                  disabled={busy}
                  onClick={() =>
                    void perform(t('cue.actions.searchAlbum'), async () =>
                      setCandidates(
                        await call<CueCandidates>('cue_candidates', {
                          id: analysis.id,
                          query,
                          offset: 0,
                        }),
                      ),
                    )
                  }
                >
                  {t('common.actions.search')}
                </Button>
                <p className="muted">
                  {candidates?.method === 'toc'
                    ? t('cue.search.approximate')
                    : candidates?.method === 'discid'
                      ? 'Disc ID'
                      : 'MusicBrainz'}
                </p>
                {candidates?.items.map((s) => (
                  <Button
                    className="cue-candidate"
                    variant={release?.id === s.id ? 'secondary' : 'outline'}
                    key={s.id}
                    disabled={busy}
                    onClick={() =>
                      void perform(t('cue.actions.loadAlbum'), () =>
                        loadRelease(s.id),
                      )
                    }
                  >
                    <span>{s.title}</span>
                    <small>
                      {[
                        s.info?.artist,
                        s.info?.air_date || s.year,
                        s.info?.countries?.join(' / '),
                        s.overview,
                      ]
                        .filter(Boolean)
                        .join(' · ')}
                    </small>
                  </Button>
                ))}
                {candidates?.next_offset != null && (
                  <Button
                    variant="outline"
                    disabled={busy}
                    onClick={() =>
                      void perform(t('cue.actions.searchAlbum'), async () => {
                        const next = await call<CueCandidates>(
                          'cue_candidates',
                          {
                            id: analysis.id,
                            query,
                            offset: candidates.next_offset,
                          },
                        )
                        setCandidates({
                          ...next,
                          items: [...candidates.items, ...next.items],
                        })
                      })
                    }
                  >
                    {t('common.actions.more')}
                  </Button>
                )}
              </>
            )}
            {step === 'organize' && (
              <div className="cue-summary">
                <h3>{title}</h3>
                <p>{artist}</p>
                <p className="muted">{t('cue.steps.organizeHint')}</p>
                <Field label={t('cue.originals.label')}>
                  <SelectField
                    value={sourceDisposition}
                    disabled={busy}
                    onValueChange={(value) => {
                      setSourceDisposition(value)
                      setSourceDispositionConfirmed(false)
                    }}
                  >
                    <SelectOption value="keep">
                      {t('cue.originals.keep')}
                    </SelectOption>
                    <SelectOption value="delete">
                      {t('cue.originals.delete')}
                    </SelectOption>
                    <SelectOption value="backup">
                      {t('cue.originals.backup')}
                    </SelectOption>
                  </SelectField>
                </Field>
                <p className="cue-format">{t('cue.originals.condition')}</p>
                {sourceDisposition === 'backup' && plan && (
                  <div className="cue-original-files">
                    <span className="cue-format">
                      {t('cue.outputs.backup')}
                    </span>
                    <FilePath path={plan.backup_root} openButton={false} />
                  </div>
                )}
                <details className="cue-original-files">
                  <summary>{t('cue.originals.files')}</summary>
                  {[
                    ...new Set([
                      path,
                      ...(analysis?.sources.map((s) => s.file.path) || []),
                    ]),
                  ].map((file) => (
                    <FilePath key={file} path={file} />
                  ))}
                </details>
                {sourceDisposition !== 'keep' && (
                  <Label className="cue-original-confirm">
                    <Checkbox
                      checked={sourceDispositionConfirmed}
                      disabled={busy}
                      onCheckedChange={(value) =>
                        setSourceDispositionConfirmed(value === true)
                      }
                    />
                    {sourceDisposition === 'delete'
                      ? t('cue.originals.confirmDelete')
                      : t('cue.originals.confirmBackup')}
                  </Label>
                )}
              </div>
            )}
          </aside>
        </div>
        <footer className="cue-footer">
          <span className="muted cue-footer-status" role="status">
            {busy
              ? t('cue.status.processing')
              : reading
                ? t('cue.status.reading')
                : sourceDisposition === 'keep'
                  ? t('cue.sources.retained')
                  : t('cue.originals.condition')}
          </span>
          <div className="cue-step-actions">
            {submitted && (
              <span role="status">{t('cue.status.submitted')}</span>
            )}
            {step !== 'parse' && !submitted && (
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => {
                  if (step === 'organize') setPlan(undefined)
                  else reset()
                }}
              >
                {t('cue.steps.back')}
              </Button>
            )}
            {analyzing && (
              <Button
                size="sm"
                variant="outline"
                onClick={() => void call('cancel_cue_analysis')}
              >
                {t('cue.actions.cancelAnalysis')}
              </Button>
            )}
            {step === 'parse' && sheet && !analyzing && (
              <Button
                disabled={
                  busy ||
                  reading ||
                  submitted ||
                  !!sheet.references?.some((r) => !r.path)
                }
                onClick={() =>
                  void perform(t('cue.actions.analyze'), async () => {
                    reset()
                    setAnalyzing(true)
                    const a = await call<CueAnalysis>('analyze_cue', {
                      request: {
                        path,
                        encoding,
                        files,
                        selected_paths: selectedPaths,
                      },
                    }).finally(() => setAnalyzing(false))
                    setAnalysis(a)
                    setTitle(a.sheet.title)
                    setArtist(a.sheet.performer)
                    setDate(a.sheet.rem.DATE || '')
                    mapTracks(a)
                    setQuery(
                      [a.sheet.title, a.sheet.performer]
                        .filter(Boolean)
                        .join(' '),
                    )
                    setCandidates(
                      await call<CueCandidates>('cue_candidates', {
                        id: a.id,
                        query: null,
                        offset: 0,
                      }),
                    )
                  })
                }
              >
                {t('cue.actions.analyze')}
              </Button>
            )}

            {step === 'album' && analysis && !submitted && (
              <>
                {!local && (
                  <Button
                    variant={release ? 'outline' : 'default'}
                    disabled={busy || submitted}
                    onClick={() => {
                      setLocal(true)
                      setRelease(undefined)
                      setTitle(analysis.sheet.title)
                      setArtist(analysis.sheet.performer)
                      mapTracks(analysis)
                    }}
                  >
                    {t('cue.actions.useLocalAlbum')}
                  </Button>
                )}
                {(local || release) && (
                  <Button
                    disabled={
                      busy ||
                      submitted ||
                      (!release && !local) ||
                      !title.trim() ||
                      tracks.some(
                        (x) => !x.title.trim() || (release && !x.remote_id),
                      )
                    }
                    onClick={() =>
                      void perform(t('cue.actions.preview'), async () =>
                        setPlan(
                          await call<Plan>('preview_cue', {
                            request: {
                              analysis_id: analysis.id,
                              release_id: release?.id || '',
                              work_id: workId || null,
                              disc,
                              title,
                              artist,
                              date,
                              tracks,
                            },
                          }),
                        ),
                      )
                    }
                  >
                    {t('cue.actions.preview')}
                  </Button>
                )}
              </>
            )}
            {step === 'organize' && (
              <ActionSplitButton
                value={mode}
                labels={{
                  view: t('common.create.view'),
                  execute: t('common.create.execute'),
                }}
                onChange={setMode}
                label={t('cue.create.mode')}
                disabled={
                  busy ||
                  !plan ||
                  submitted ||
                  (sourceDisposition !== 'keep' &&
                    !sourceDispositionConfirmed) ||
                  plan.rows.every((r) => r.action === 'skip') ||
                  plan.rows.some(
                    (r) =>
                      r.action !== 'skip' &&
                      (!['copy', 'replace'].includes(r.action) ||
                        (r.warnings.length > 0 && !r.reviewed)),
                  )
                }
                onAction={() =>
                  void perform(t('cue.actions.createTask'), async () => {
                    if (!plan) return
                    const created = await call<Plan>('submit_cue', {
                      id: plan.id,
                      sourceDisposition,
                      sourceDispositionConfirmed,
                      choices: plan.rows.map((r) => ({
                        id: r.id,
                        action: r.action,
                        operation: 'split',
                        reviewed: r.reviewed,
                      })),
                    })
                    setSubmitted(true)
                    setPlan(undefined)
                    await onPlan(created, mode)
                    onClose()
                  })
                }
              />
            )}
          </div>
        </footer>
      </DialogContent>
    </Dialog>
  )
}
