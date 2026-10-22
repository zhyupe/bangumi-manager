import {
  FilePlusIcon,
  ArchiveIcon as FolderPlusIcon,
  InfoCircledIcon,
  TrashIcon,
} from '@radix-ui/react-icons'
import { open } from '@tauri-apps/plugin-dialog'
import { useEffect, useMemo, useRef, useState } from 'react'
import { ActionSplitButton } from '@/components/action-split-button'
import { Disclosure, DisclosureTrigger } from '@/components/disclosure'
import { EpisodeSelect } from '@/components/episode-select'
import { Field } from '@/components/field'
import { FilePath, OpenPathButton } from '@/components/file-path'
import { Pages } from '@/components/pagination'
import { ProbeInfo } from '@/components/probe-info'
import { SelectField, SelectOption } from '@/components/select-field'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { WorkSelect } from '@/components/work-select'
import NfoSync from '@/features/metadata/NfoSync'
import CueImportDialog from '@/features/cue/CueImportDialog'
import {
  commonKeywords,
  suggestSourceImport,
} from '@/features/metadata/source-search'
import { useFileSelection } from '@/hooks/use-file-selection'
import { useRememberedAction } from '@/hooks/use-remembered-action'
import { t } from '@/i18n'
import { call } from '@/lib/api'
import { bytes, natural } from '@/lib/format'
import { inferParts } from '@/lib/media-parts'
import { isSubtitle } from '@/lib/subtitles'
import type { Mapping, Plan, Probe, Run, SourceFile, Work } from '@/types'
import {
  batchMatch,
  directoryExtraMappings,
  extendRange,
  numbers,
  standaloneSources,
} from './matching'
import { episodeRanges, seasonMatchSummary } from './season-matching'

export type CreateMode = 'create' | 'view' | 'execute'
const createLabels = {
  get create() {
    return t('common.create.only')
  },
  get view() {
    return t('common.create.view')
  },
  get execute() {
    return t('common.create.execute')
  },
}
interface Props {
  defaultOperation: string
  cueSourceDisposition?: 'keep' | 'delete' | 'backup'
  files: SourceFile[]
  works: Work[]
  workId: string
  onWork: (id: string) => void
  run: Run
  busy: boolean
  onScan: (paths: string[]) => Promise<void>
  onReload: () => Promise<void>
  onAdd: (target: { kind: string; keyword: string }) => void
  onPlan: (plan: Plan, mode: CreateMode) => Promise<void>
  mappings: Record<string, Mapping>
  onMappings: (m: Record<string, Mapping>) => void
}
export default function SourcePane({
  defaultOperation,
  cueSourceDisposition = 'keep',
  files: scannedFiles,
  works,
  workId,
  onWork,
  run,
  busy,
  onScan,
  onReload,
  onAdd,
  onPlan,
  mappings,
  onMappings,
}: Props) {
  const [selected, setSelected] = useState<Set<string>>(new Set()),
    [filter, setFilter] = useState(''),
    [group, setGroup] = useState(''),
    [onlyVideo, setOnlyVideo] = useState(false),
    [page, setPage] = useState(1),
    [path, setPath] = useState('')
  const [mode, setMode] = useState('number'),
    [token, setToken] = useState(0),
    [start, setStart] = useState(1),
    [offset, setOffset] = useState(0),
    [season, setSeason] = useState(1),
    [binding, setBinding] = useState('')
  const [createMode, setCreateMode] = useRememberedAction<CreateMode>(
    'bangumi.createMode',
    ['create', 'view', 'execute'],
    'view',
  )
  const creating = useRef(false)
  const [partialMode, setPartialMode] = useState<CreateMode | null>(null)
  const autoAssignedExtras = useRef(new Set<string>())
  const [cuePaths, setCuePaths] = useState<string[]>([])
  const [chosenCue, setChosenCue] = useState('')
  const [probes, setProbes] = useState<Record<string, Probe>>({})
  const [matchResult, setMatchResult] = useState<{
    matched: number
    total: number
  } | null>(null)
  const files = useMemo(
    () => standaloneSources(scannedFiles, mappings),
    [scannedFiles, mappings],
  )
  useEffect(() => {
    const paths = new Set(files.map((f) => f.path))
    setSelected((previous) =>
      [...previous].every((path) => paths.has(path))
        ? previous
        : new Set([...previous].filter((path) => paths.has(path))),
    )
  }, [files])
  const selection = useFileSelection(selected, setSelected)
  const work = works.find((w) => w.id === workId)
  const chapterLabel =
    work?.kind === 'music'
      ? t('organize.mapping.track')
      : work?.kind === 'movie'
        ? t('organize.mapping.content')
        : t('organize.mapping.episode')
  useEffect(() => {
    setBinding('')
    setSeason(
      work?.episodes.find((e) => e.enabled && e.season > 0)?.season ?? 1,
    )
  }, [workId])
  const shown = useMemo(
    () =>
      files
        .filter(
          (f) =>
            (!onlyVideo || f.media) &&
            (!group || f.parent === group) &&
            (!filter ||
              `${f.name} ${f.parent}`
                .toLowerCase()
                .includes(filter.toLowerCase())),
        )
        .sort((a, b) => natural(a.path, b.path)),
    [files, filter, group, onlyVideo],
  )
  const selectedFiles = useMemo(
    () => files.filter((f) => selected.has(f.path)),
    [files, selected],
  )
  useEffect(() => {
    if (!work || busy) return
    const additions = directoryExtraMappings(files, work.id).filter(
      (mapping) => {
        const key = `${work.id}\0${mapping.source}`
        if (autoAssignedExtras.current.has(key)) return false
        autoAssignedExtras.current.add(key)
        return !mappings[mapping.source]
      },
    )
    if (additions.length) {
      onMappings({
        ...mappings,
        ...Object.fromEntries(
          additions.map((mapping) => [mapping.source, mapping]),
        ),
      })
    }
  }, [files, work, busy, mappings, onMappings])
  const numericFields = useMemo(() => {
    const examples: number[] = []
    for (const file of selectedFiles)
      numbers(file.name).forEach((number, index) => {
        examples[index] ??= number
      })
    return examples
  }, [selectedFiles])
  useEffect(() => {
    setToken((previous) =>
      Math.min(previous, Math.max(0, numericFields.length - 1)),
    )
  }, [numericFields.length])
  useEffect(() => {
    setMatchResult(null)
  }, [
    selected,
    workId,
    season,
    binding,
    mode,
    token,
    start,
    offset,
    scannedFiles,
  ])
  const nfoSources = useMemo(() => {
    const selectedNfos = new Set(
      selectedFiles
        .filter((f) => /\.nfo$/i.test(f.path))
        .map((f) => f.path.replace(/\.nfo$/i, '').toLowerCase()),
    )
    return scannedFiles.filter(
      (f) =>
        f.media &&
        (selected.has(f.path) ||
          selectedNfos.has(f.path.replace(/\.[^./\\]+$/, '').toLowerCase())),
    )
  }, [scannedFiles, selectedFiles, selected])

  const keywords = useMemo(() => commonKeywords(selectedFiles), [selectedFiles])
  const addFromSelection = (keyword?: string) => {
    const target = suggestSourceImport(selectedFiles, work?.kind)
    onAdd(keyword === undefined ? target : { ...target, keyword })
  }
  const groups = useMemo(
    () => [...new Set(files.map((f) => f.parent))].sort(natural),
    [files],
  )
  const chosen = selectedFiles.filter((f) => mappings[f.path])
  const mediaSummary = useMemo(
    () => seasonMatchSummary(work, scannedFiles, mappings, season, binding),
    [work, scannedFiles, mappings, season, binding],
  )
  const subtitleSummary = useMemo(
    () => seasonMatchSummary(work, files, mappings, season, binding, true),
    [work, files, mappings, season, binding],
  )
  const hasSubtitles = selectedFiles.some(
    (file) => isSubtitle(file.name) && !mappings[file.path]?.extra,
  )
  const subtitleOnly =
    hasSubtitles &&
    work?.kind !== 'music' &&
    selectedFiles.every(
      (file) => isSubtitle(file.name) || mappings[file.path]?.extra,
    )
  const seasonSummary = subtitleOnly ? subtitleSummary : mediaSummary
  const available =
    work?.episodes
      .filter((e) => e.enabled)
      .sort((a, b) => a.season - b.season || a.number - b.number) || []
  const choose = (directory: boolean) =>
    run(t('organize.files.choose'), async () => {
      const result = await open({ directory, multiple: true })
      if (result) {
        const paths = Array.isArray(result) ? result : [result]
        await onScan(paths)
        const cues = paths.filter((p) => /\.cue$/i.test(p))
        if (cues.length) {
          setChosenCue(cues[0])
          setSelected((previous) => new Set([...previous, ...paths]))
        }
      }
    })
  const toggle = (p: string, v: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev)
      v ? next.add(p) : next.delete(p)
      return next
    })
  const update = (file: SourceFile, value: string) => {
    setMatchResult(null)
    const next = { ...mappings }
    if (!value) delete next[file.path]
    else if (work)
      next[file.path] = {
        source: file.path,
        work_id: work.id,
        episode_ids: value === 'extra' ? [] : [value],
        extra: value === 'extra',
        part:
          value === 'extra' || work.kind === 'music'
            ? 0
            : (mappings[file.path]?.part ??
              inferParts(files, work.kind).get(file.path) ??
              0),
      }
    onMappings(next)
  }
  const createTask = async (mode: CreateMode, allowPartial = false) => {
    if (creating.current) return
    if (chosen.length && chosen.length !== selectedFiles.length && !allowPartial) {
      setPartialMode(mode)
      return
    }
    setPartialMode(null)
    creating.current = true
    try {
      await run(t('organize.tasks.create'), async () => {
        if (!chosen.length)
          throw new Error(t('organize.tasks.unmatchedSelection'))
        const plan = await call<Plan>('create_plan', {
          mappings: chosen.map((f) => mappings[f.path]),
        })
        const submitted = new Set(chosen.map((f) => f.path))
        setSelected(
          (prev) => new Set([...prev].filter((path) => !submitted.has(path))),
        )
        await onPlan(plan, mode)
      })
    } finally {
      creating.current = false
    }
  }
  return (
    <section className="source-panel">
      <Dialog open={partialMode !== null} onOpenChange={open => { if (!open) setPartialMode(null) }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('organize.tasks.partial.title')}</DialogTitle>
            <DialogDescription>
              {t('organize.tasks.partial.description', {matched: chosen.length, unmatched: selectedFiles.length - chosen.length})}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPartialMode(null)}>{t('common.actions.cancel')}</Button>
            <Button disabled={busy || !chosen.length} onClick={() => {
              if (partialMode) void createTask(partialMode, true)
            }}>{t('organize.tasks.partial.confirm')}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {cuePaths.length > 0 && (
        <CueImportDialog
          path={cuePaths[0]}
          defaultSourceDisposition={cueSourceDisposition}
          selectedPaths={selectedFiles
            .filter((f) => f.media)
            .map((f) => f.path)}
          works={works}
          run={run}
          onClose={() => setCuePaths([])}
          onPlan={async (plan, mode) => {
            await onReload()
            await onPlan(plan, mode)
          }}
        />
      )}
      <div className="source-toolbar">
        {files.length > 0 && (
          <div className="filter-row">
            <SelectField
              aria-label={t('organize.filters.directory')}
              contentClassName="source-directory-options"
              title={group || undefined}
              value={group}
              onValueChange={(value) => {
                setGroup(value)
                setPage(1)
              }}
            >
              <SelectOption value="">
                {t('organize.filters.allDirectories', { count: groups.length })}
              </SelectOption>
              {groups.map((g) => (
                <SelectOption key={g} value={g}>
                  {g}
                </SelectOption>
              ))}
            </SelectField>
            {group && <OpenPathButton path={group} />}
            <Input
              aria-label={t('organize.filters.label')}
              value={filter}
              onChange={(e) => {
                setFilter(e.target.value)
                setPage(1)
              }}
              placeholder={t('organize.filters.filename')}
            />
            <Label className="check-label">
              <Checkbox
                checked={onlyVideo}
                onCheckedChange={(v) => {
                  setOnlyVideo(v === true)
                  setPage(1)
                }}
              />
              {t('organize.filters.mediaOnly')}
            </Label>
          </div>
        )}
        <div className="button-row">
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => void choose(false)}
          >
            <FilePlusIcon />
            {t('common.files.file')}
          </Button>
          <Button disabled={busy} onClick={() => void choose(true)}>
            <FolderPlusIcon />
            {t('common.files.directory')}
          </Button>
          <Popover>
            <PopoverTrigger asChild>
              <Button variant="secondary">{t('common.files.path')}</Button>
            </PopoverTrigger>
            <PopoverContent style={{ width: '480px' }}>
              <h3>{t('organize.scan.pathTitle')}</h3>
              <form
                className="search-row"
                onSubmit={(e) => {
                  e.preventDefault()
                  void onScan([path])
                }}
              >
                <Input
                  aria-label={t('organize.scan.pathLabel')}
                  placeholder={t('organize.scan.pathPlaceholder')}
                  value={path}
                  onChange={(e) => setPath(e.target.value)}
                />
                <OpenPathButton path={path} />
                <Button
                  variant="secondary"
                  disabled={busy || !path.trim()}
                  type="submit"
                >
                  {t('organize.scan.pathAction')}
                </Button>
              </form>
            </PopoverContent>
          </Popover>
          <Button
            size="sm"
            data-tone="gray"
            variant="ghost"
            disabled={busy}
            onClick={() =>
              void run(t('organize.files.clearAction'), async () => {
                await call('clear_sources')
                setSelected(new Set())
                onMappings({})
                await onReload()
              })
            }
          >
            <TrashIcon />
            {t('organize.files.clear')}
          </Button>
        </div>
      </div>
      {files.length === 0 ? (
        <div className="drop-area">
          <FolderPlusIcon />
          <h3>{t('organize.drop.placeholder')}</h3>
        </div>
      ) : (
        <div className="source-workspace">
          <div className="source-file-browser">
            <div className="selection-bar">
              <Button
                size="sm"
                variant="secondary"
                onClick={() =>
                  setSelected(
                    (prev) => new Set([...prev, ...shown.map((f) => f.path)]),
                  )
                }
              >
                {t('common.selection.all')}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setSelected(new Set())}
              >
                {t('common.selection.clear')}
              </Button>
              <span>
                {t('organize.selection.summary', {
                  count: selectedFiles.length,
                  size: bytes(
                    selectedFiles.reduce((n, f) => n + f.signature.size, 0),
                  ),
                })}
              </span>
              <small className="selection-help">
                {t('organize.selection.hint')}
              </small>
            </div>
            <div
              className="table-wrap file-viewport"
              ref={selection.viewport}
              {...selection.handlers}
            >
              <Table className="file-table">
                <TableHeader>
                  <TableRow>
                    <TableHead className="check-col">
                      <Checkbox
                        aria-label={t('organize.selection.filtered')}
                        checked={
                          shown.length > 0 &&
                          shown.every((f) => selected.has(f.path))
                            ? true
                            : shown.some((f) => selected.has(f.path))
                              ? 'indeterminate'
                              : false
                        }
                        onCheckedChange={(v) =>
                          setSelected((prev) => {
                            const next = new Set(prev)
                            shown.forEach((f) =>
                              v === true
                                ? next.add(f.path)
                                : next.delete(f.path),
                            )
                            return next
                          })
                        }
                      />
                    </TableHead>
                    <TableHead>{t('organize.columns.filename')}</TableHead>
                    <TableHead>{t('common.files.size')}</TableHead>
                    <TableHead>{chapterLabel}</TableHead>
                    <TableHead title={t('organize.range.hint')}>
                      {t('organize.range.label')}
                    </TableHead>
                    <TableHead>{t('organize.parts.label')}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {shown.slice((page - 1) * 80, page * 80).map((file) => {
                    const mapping = mappings[file.path]
                    const belongs = mapping?.work_id === workId
                    const first =
                      belongs && mapping
                        ? available.find(
                            (ep) => ep.id === mapping.episode_ids[0],
                          )
                        : undefined
                    const rangeEnabled =
                      !!first && !mapping.extra && work?.kind === 'tv'
                    const ends = rangeEnabled
                      ? available.filter(
                          (ep) =>
                            ep.binding_id === first.binding_id &&
                            ep.season === first.season &&
                            ep.number > first.number,
                        )
                      : []
                    return (
                      <TableRow
                        key={file.path}
                        data-source={file.path}
                        aria-selected={selection.selected.has(file.path)}
                        tabIndex={0}
                        onKeyDown={(e) => {
                          if (
                            e.target === e.currentTarget &&
                            (e.key === ' ' || e.key === 'Enter')
                          ) {
                            e.preventDefault()
                            toggle(file.path, !selected.has(file.path))
                          }
                        }}
                        className={
                          selection.selected.has(file.path)
                            ? 'selected-row'
                            : ''
                        }
                      >
                        <TableCell>
                          <Checkbox
                            aria-label={t('common.selection.item', {
                              filename: file.name,
                            })}
                            checked={selection.selected.has(file.path)}
                            onCheckedChange={(v) =>
                              toggle(file.path, v === true)
                            }
                          />
                        </TableCell>
                        <TableCell>
                          <div className="file-title-row">
                            <div className="file-name" title={file.path}>
                              {file.name}
                            </div>
                            <OpenPathButton path={file.path} />
                            <Popover>
                              <PopoverTrigger asChild>
                                <Button
                                  size="icon-xs"
                                  variant="ghost"
                                  aria-label={t('organize.file.details', {
                                    filename: file.name,
                                  })}
                                >
                                  <InfoCircledIcon />
                                </Button>
                              </PopoverTrigger>
                              <PopoverContent
                                className="file-info-popover"
                                style={{
                                  maxWidth: 'min(420px, calc(100vw - 32px))',
                                }}
                              >
                                <strong>{file.name}</strong>
                                <p>
                                  <FilePath
                                    path={file.path}
                                    openButton={false}
                                  />
                                </p>
                                <p>{bytes(file.signature.size)}</p>
                                <ProbeInfo
                                  probe={probes[file.path] || file.probe}
                                />
                                {file.media && (
                                  <Button
                                    size="sm"
                                    variant="secondary"
                                    disabled={busy}
                                    onClick={() =>
                                      void run(
                                        t('media.actions.readProbe'),
                                        async () => {
                                          const p = await call<Probe>(
                                            'probe_file',
                                            { path: file.path },
                                          )
                                          setProbes((prev) => ({
                                            ...prev,
                                            [file.path]: p,
                                          }))
                                        },
                                      )
                                    }
                                  >
                                    {t('media.actions.refreshProbe')}
                                  </Button>
                                )}
                              </PopoverContent>
                            </Popover>
                          </div>
                          <div className="file-subline">
                            {isSubtitle(file.name) && (
                              <Badge tone="gray">
                                {t('common.files.subtitle')}
                              </Badge>
                            )}
                            <FilePath
                              path={file.parent}
                              truncate
                              openButton={false}
                            />
                            {file.hint !== 'episode' && (
                              <Badge
                                tone={
                                  file.hint === 'special' ? 'amber' : 'gray'
                                }
                              >
                                {file.extra_relative_path
                                  ? t('settings.storage.extrasRoot')
                                  : file.hint === 'special'
                                    ? t('organize.file.specialHint')
                                    : t('organize.file.extraHint')}
                              </Badge>
                            )}
                          </div>
                        </TableCell>
                        <TableCell className="nowrap">
                          {bytes(file.signature.size)}
                        </TableCell>
                        <TableCell>
                          <EpisodeSelect
                            episodes={available}
                            kind={work?.kind}
                            label={`${file.name} ${chapterLabel}`}
                            disabled={!work}
                            value={
                              belongs
                                ? mapping.extra
                                  ? 'extra'
                                  : mapping.episode_ids[0] || ''
                                : ''
                            }
                            onValueChange={(value) => update(file, value)}
                          />
                          {mapping && !belongs && (
                            <small>
                              {t('organize.mapping.matchedTo', {
                                title: works.find(
                                  (w) => w.id === mapping.work_id,
                                )?.title,
                              })}
                            </small>
                          )}
                        </TableCell>
                        <TableCell>
                          {work && work.kind !== 'tv' ? (
                            <span className="muted">
                              {work.kind === 'music'
                                ? t('organize.file.track')
                                : t('organize.file.movie')}
                            </span>
                          ) : (
                            <EpisodeSelect
                              end
                              kind={work?.kind}
                              episodes={ends}
                              label={t('organize.mapping.rangeEndLabel', {
                                filename: file.name,
                              })}
                              disabled={!rangeEnabled || !!mapping?.part}
                              value={
                                rangeEnabled && mapping.episode_ids.length > 1
                                  ? mapping.episode_ids.at(-1)!
                                  : ''
                              }
                              onValueChange={(value) => {
                                if (work && rangeEnabled)
                                  onMappings({
                                    ...mappings,
                                    [file.path]: {
                                      ...mapping,
                                      episode_ids: extendRange(
                                        work,
                                        mapping.episode_ids[0],
                                        value,
                                      ),
                                    },
                                  })
                              }}
                            />
                          )}
                          {rangeEnabled && mapping.episode_ids.length > 1 && (
                            <small>
                              {t('organize.range.count', {
                                count: mapping.episode_ids.length,
                              })}
                            </small>
                          )}
                        </TableCell>
                        <TableCell>
                          {work?.kind !== 'music' ? (
                            <SelectField
                              aria-label={t('organize.parts.numberLabel', {
                                filename: file.name,
                              })}
                              className="part-select"
                              disabled={
                                !belongs ||
                                !mapping ||
                                mapping.extra ||
                                mapping.episode_ids.length !== 1
                              }
                              value={String(mapping?.part ?? 0)}
                              onValueChange={(value) => {
                                if (mapping)
                                  onMappings({
                                    ...mappings,
                                    [file.path]: {
                                      ...mapping,
                                      part: Number(value),
                                    },
                                  })
                              }}
                            >
                              <SelectOption value="0">
                                {t('organize.parts.none')}
                              </SelectOption>
                              {Array.from({ length: 8 }, (_, i) => (
                                <SelectOption key={i + 1} value={String(i + 1)}>
                                  Part {i + 1}
                                </SelectOption>
                              ))}
                            </SelectField>
                          ) : (
                            <span className="muted">—</span>
                          )}
                        </TableCell>
                      </TableRow>
                    )
                  })}
                </TableBody>
              </Table>
              <div
                ref={selection.box}
                className="selection-box"
                style={{ display: 'none' }}
                aria-hidden="true"
              />
            </div>

            <Pages page={page} count={shown.length} onChange={setPage} />
          </div>
          <aside
            className="source-sidebar"
            aria-label={t('organize.mapping.title')}
          >
            <div className="source-sidebar-body">
              {selectedFiles.some((f) => /\.cue$/i.test(f.path)) && (
                <div className="cue-source-choice">
                  <SelectField
                    aria-label={t('cue.fields.file')}
                    value={
                      selectedFiles.some(
                        (f) => f.path === chosenCue && /\.cue$/i.test(f.path),
                      )
                        ? chosenCue
                        : selectedFiles.find((f) => /\.cue$/i.test(f.path))
                            ?.path
                    }
                    disabled={busy}
                    onValueChange={setChosenCue}
                  >
                    {selectedFiles
                      .filter((f) => /\.cue$/i.test(f.path))
                      .map((f) => (
                        <SelectOption key={f.path} value={f.path}>
                          {f.name}
                        </SelectOption>
                      ))}
                  </SelectField>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={busy}
                    onClick={() => {
                      const file =
                        selectedFiles.find(
                          (f) => f.path === chosenCue && /\.cue$/i.test(f.path),
                        ) || selectedFiles.find((f) => /\.cue$/i.test(f.path))
                      if (file) setCuePaths([file.path])
                    }}
                  >
                    {t('cue.actions.analyze')}
                  </Button>
                </div>
              )}
              {hasSubtitles && (
                <p className="muted">{t('organize.subtitles.importHint')}</p>
              )}
              <NfoSync
                files={nfoSources}
                mappings={mappings}
                busy={busy}
                run={run}
                onSynced={async (imported) => {
                  setMatchResult(null)
                  onMappings({
                    ...mappings,
                    ...Object.fromEntries(imported.map((m) => [m.source, m])),
                  })
                  await onReload()
                  if (imported[0]) onWork(imported[0].work_id)
                }}
              />
              <div className="mapping-controls">
                <div className="inline-fields">
                  <Field label={t('organize.mapping.work')}>
                    <WorkSelect
                      works={works}
                      value={workId}
                      disabled={busy}
                      onValueChange={(value) => {
                        onWork(value)
                        setBinding('')
                        const w = works.find((w) => w.id === value)
                        if (w) setSeason(w.bindings.at(-1)?.season ?? 1)
                      }}
                    />
                    <Button
                      variant="secondary"
                      disabled={busy}
                      onClick={() => addFromSelection()}
                    >
                      {t('library.actions.addWork')}
                    </Button>
                  </Field>
                  {work && work.kind !== 'movie' && (
                    <Field
                      label={
                        work.kind === 'music'
                          ? t('organize.mapping.trackSource')
                          : t('organize.mapping.episodeSource')
                      }
                    >
                      <SelectField
                        value={binding}
                        onValueChange={(value) => {
                          setBinding(value)
                          setSeason(
                            work.bindings.find((b) => b.id === value)?.season ??
                              season,
                          )
                        }}
                      >
                        <SelectOption value="">
                          {t('organize.mapping.allSources')}
                        </SelectOption>
                        {work.bindings.map((b) => (
                          <SelectOption key={b.id} value={b.id}>
                            {b.source_title} · {b.provider} #{b.remote_id}
                          </SelectOption>
                        ))}
                      </SelectField>
                    </Field>
                  )}
                </div>
                {work && work.kind !== 'movie' && (
                  <div className="batch-row">
                    <Label className="season-field">
                      {work.kind === 'music'
                        ? t('common.units.disc')
                        : t('common.units.season')}{' '}
                      <SelectField
                        aria-label={
                          work.kind === 'music'
                            ? t('organize.mapping.discLabel')
                            : t('organize.mapping.seasonLabel')
                        }
                        value={season}
                        onValueChange={(value) => setSeason(Number(value))}
                      >
                        {[...new Set(work.episodes.map((e) => e.season))]
                          .sort((a, b) => a - b)
                          .map((n) => (
                            <SelectOption value={n} key={n}>
                              {String(n).padStart(2, '0')} ·{' '}
                              {work.seasons?.find((s) => s.number === n)
                                ?.title ||
                                (n === 0 ? t('common.media.special') : '')}
                            </SelectOption>
                          ))}
                      </SelectField>
                    </Label>
                    <div
                      className="season-match-summary"
                      role="region"
                      aria-label={
                        work.kind === 'music'
                          ? t('organize.coverage.discLabel')
                          : t('organize.coverage.seasonLabel')
                      }
                      aria-live="polite"
                      aria-atomic="true"
                    >
                      {seasonSummary.total ? (
                        <>
                          <div className="season-match-counts">
                            <strong>
                              {t('organize.coverage.total', {
                                count: seasonSummary.total,
                                unit:
                                  work.kind === 'music'
                                    ? t('common.units.track')
                                    : t('common.units.episode'),
                              })}
                            </strong>
                            <Badge
                              tone={
                                seasonSummary.missing.length ? 'gray' : 'green'
                              }
                            >
                              {t('organize.coverage.matched', {
                                count: seasonSummary.matched,
                                unit:
                                  work.kind === 'music'
                                    ? t('common.units.track')
                                    : t('common.units.episode'),
                              })}
                            </Badge>
                          </div>
                          <p>
                            {seasonSummary.missing.length
                              ? t('organize.coverage.missing', {
                                  count: seasonSummary.missing.length,
                                  unit:
                                    work.kind === 'music'
                                      ? t('common.units.track')
                                      : t('common.units.episode'),
                                  episodes: episodeRanges(
                                    seasonSummary.missing,
                                    work.kind === 'music'
                                      ? t('organize.coverage.trackPrefix')
                                      : 'EP',
                                  ),
                                })
                              : t('organize.coverage.complete')}
                          </p>
                        </>
                      ) : (
                        <p>
                          {work.kind === 'music'
                            ? t('organize.coverage.emptyDisc')
                            : t('organize.coverage.emptySeason')}
                        </p>
                      )}
                      <small>
                        {subtitleOnly
                          ? t('organize.subtitles.coverageScope')
                          : t('organize.coverage.scope')}
                      </small>
                      {hasSubtitles &&
                        !subtitleOnly &&
                        work.kind !== 'music' && (
                          <p>
                            {t('organize.subtitles.coverage', {
                              matched: subtitleSummary.matched,
                              missing: subtitleSummary.missing.length
                                ? episodeRanges(subtitleSummary.missing)
                                : t('common.values.none'),
                            })}
                          </p>
                        )}
                    </div>
                    <SelectField
                      aria-label={t('organize.matching.mode')}
                      value={mode}
                      onValueChange={(value) => setMode(value)}
                    >
                      <SelectOption value="sequence">
                        {t('organize.matching.sequence')}
                      </SelectOption>
                      <SelectOption value="name">
                        {t('organize.matching.name')}
                      </SelectOption>
                      <SelectOption value="number">
                        {t('organize.matching.number')}
                      </SelectOption>
                    </SelectField>
                    {mode === 'number' ? (
                      <>
                        <Label>
                          {t('organize.matching.numberPosition')}
                          <SelectField
                            value={token}
                            onValueChange={(value) => setToken(Number(value))}
                          >
                            {Array.from(
                              {
                                length: Math.max(1, numericFields.length),
                              },
                              (_, n) => (
                                <SelectOption key={n} value={n}>
                                  {n + 1}{' '}
                                  {numericFields[n] !== undefined
                                    ? t('organize.matching.numberExample', {
                                        number: numericFields[n],
                                      })
                                    : ''}
                                </SelectOption>
                              ),
                            )}
                          </SelectField>
                        </Label>
                        <Label>
                          {t('organize.matching.offset')}
                          <Input
                            aria-label={t('organize.matching.offsetLabel')}
                            className="number-input"
                            type="number"
                            value={offset}
                            onChange={(e) => setOffset(Number(e.target.value))}
                          />
                        </Label>
                      </>
                    ) : mode === 'sequence' ? (
                      <Label>
                        {t('organize.matching.start')}
                        <Input
                          aria-label={t('organize.matching.startLabel')}
                          className="number-input"
                          type="number"
                          min="1"
                          value={start}
                          onChange={(e) => setStart(Number(e.target.value))}
                        />
                      </Label>
                    ) : (
                      <span className="muted">
                        {t('organize.matching.nameHint', {
                          kind:
                            work.kind === 'music'
                              ? t('common.media.track')
                              : t('common.media.episode'),
                        })}
                      </span>
                    )}
                  </div>
                )}
                <div className="button-row">
                  <Button
                    variant="secondary"
                    disabled={!work || !selectedFiles.length || busy}
                    onClick={() => {
                      if (!work) return
                      const rows = batchMatch(
                        selectedFiles,
                        work,
                        season,
                        binding,
                        mode,
                        token,
                        start,
                        offset,
                      )
                      onMappings({
                        ...Object.fromEntries(
                          Object.entries(mappings).filter(
                            ([path]) => !selected.has(path),
                          ),
                        ),
                        ...Object.fromEntries(rows.map((r) => [r.source, r])),
                      })
                      setMatchResult({
                        matched: rows.length,
                        total: selectedFiles.length,
                      })
                    }}
                  >
                    {t('organize.matching.apply')}
                  </Button>
                  <Button
                    variant="ghost"
                    disabled={!work || !selectedFiles.length || busy}
                    onClick={() => {
                      setMatchResult(null)
                      if (work)
                        onMappings({
                          ...mappings,
                          ...Object.fromEntries(
                            selectedFiles.map((f) => [
                              f.path,
                              {
                                source: f.path,
                                work_id: work.id,
                                episode_ids: [],
                                extra: true,
                              },
                            ]),
                          ),
                        })
                    }}
                  >
                    {t('organize.matching.markExtras')}
                  </Button>
                </div>
                {matchResult && (
                  <Alert role="status" aria-live="polite">
                    <AlertDescription>
                      <p>
                        {t('organize.coverage.selection', {
                          matched: matchResult.matched,
                          total: matchResult.total,
                        })}
                      </p>
                      {matchResult.matched < matchResult.total && (
                        <p>
                          {mode === 'number' && !numericFields.length
                            ? t('organize.matching.noNumbers')
                            : t('organize.matching.ambiguous')}
                        </p>
                      )}
                    </AlertDescription>
                  </Alert>
                )}
                {selectedFiles.some((file) => mappings[file.path]?.part) && (
                  <p className="muted">{t('organize.parts.hint')}</p>
                )}
              </div>
              {selectedFiles.length > 0 && (
                <Disclosure className="source-keywords">
                  <DisclosureTrigger>
                    {t('database.keywords.title')}
                  </DisclosureTrigger>
                  <div className="keyword-strip">
                    {keywords.map((k) => (
                      <Button
                        key={k.text}
                        onClick={() => addFromSelection(k.text)}
                        title={t('database.keywords.fileCount', {
                          count: k.count,
                        })}
                        variant="ghost"
                      >
                        {k.text}
                      </Button>
                    ))}
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() =>
                        addFromSelection(
                          keywords
                            .slice(0, 6)
                            .map((k) => k.text)
                            .join(' '),
                        )
                      }
                    >
                      {t('database.keywords.combine')}
                    </Button>
                  </div>
                </Disclosure>
              )}
            </div>
            <div className="source-plan-actions">
              <div>
                <strong>
                  {t('organize.coverage.selection', {
                    matched: chosen.length,
                    total: selectedFiles.length,
                  })}
                </strong>
              </div>
              <ActionSplitButton
                value={createMode}
                labels={createLabels}
                disabled={busy || !chosen.length}
                switchDisabled={busy}
                label={t('organize.tasks.switchAction')}
                onAction={() => void createTask(createMode)}
                onChange={setCreateMode}
              />
              <small>
                {t('organize.tasks.operationSummary', {
                  operation:
                    {
                      copy: t('common.operations.copy'),
                      move: t('common.operations.move'),
                      hardlink: t('common.operations.hardlink'),
                      symlink: t('common.operations.symlink'),
                    }[defaultOperation] || t('common.operations.copy'),
                })}
              </small>
            </div>
          </aside>
        </div>
      )}
    </section>
  )
}
