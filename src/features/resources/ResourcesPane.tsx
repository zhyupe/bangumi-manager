import { ArrowDownIcon, ArrowUpIcon } from '@radix-ui/react-icons'
import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Disclosure, DisclosureTrigger } from '@/components/disclosure'
import { Empty } from '@/components/empty-state'
import { Field } from '@/components/field'
import { FilePath } from '@/components/file-path'
import MediaDetails from '@/components/media-details'
import type { Notify } from '@/components/notifications'
import { Pages } from '@/components/pagination'
import { SelectField, SelectOption } from '@/components/select-field'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { formatDate, t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import { call, desktop } from '@/lib/api'
import { bytes, filename } from '@/lib/format'
import { episodePosition } from '@/lib/media'
import type {
  BackupRecord,
  Probe,
  ReplacementTarget,
  ResourceFile,
  Run,
  Work,
} from '@/types'
import {
  defaultFilters,
  filterResources,
  type ResourceFilters,
  type ResourceSort,
  resourceStatus,
  sortResources,
  statusLabels,
} from './resource-model'
import './resources.css'

export default function ResourcesPane({
  active,
  works,
  tabsContainer,
  run,
  busy,
  onReload,
  onReplace,
  onRename,
  onCatalog,
  notify,
}: {
  active: boolean
  works: Work[]
  tabsContainer: HTMLDivElement | null
  run: Run
  busy: boolean
  onReload: () => Promise<void>
  onReplace: (target: ReplacementTarget) => void
  onRename: (paths: string[]) => void
  onCatalog: (id?: string) => void
  notify: Notify
}) {
  const [tab, setTab] = useState('media')
  const [files, setFiles] = useState<ResourceFile[]>([])
  const [backups, setBackups] = useState<BackupRecord[]>([])
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [backupSelected, setBackupSelected] = useState<Set<string>>(new Set())
  const [backupTask, setBackupTask] = useState('all')
  const [cleanup, setCleanup] = useState<BackupRecord[] | null>(null)
  const [detail, setDetail] = useState<ResourceFile | null>(null)
  const [page, setPage] = useState(1)
  const [backupPage, setBackupPage] = useState(1)
  const [probing, setProbing] = useState(false)
  const [loading, setLoading] = useState(desktop)
  const [loadError, setLoadError] = useState('')
  const [progress, setProgress] = useState({ done: 0, total: 0 })
  const [sort, setSort] = useState<{ by: ResourceSort; desc: boolean }>({
    by: 'size',
    desc: true,
  })
  const [filters, setFilters] = useState<ResourceFilters>(() => {
    try {
      return {
        ...defaultFilters,
        ...JSON.parse(localStorage.getItem('bangumi.resource-filters') || '{}'),
      }
    } catch {
      return defaultFilters
    }
  })
  const reloadVersion = useRef(0)
  const reload = useCallback(async () => {
    const version = ++reloadVersion.current
    setLoading(true)
    setLoadError('')
    try {
      const [media, backup] = await Promise.all([
        call<ResourceFile[]>('list_resources'),
        call<BackupRecord[]>('list_backups'),
      ])
      if (version !== reloadVersion.current) return
      setFiles(media)
      setBackups(backup)
      setSelected(
        (s) =>
          new Set([...s].filter((path) => media.some((f) => f.path === path))),
      )
      setBackupSelected(
        (s) =>
          new Set(
            [...s].filter((id) =>
              backup.some(
                (b) => b.id === id && ['available', 'error'].includes(b.state),
              ),
            ),
          ),
      )
    } catch (e) {
      if (version === reloadVersion.current) setLoadError(String(e))
      throw e
    } finally {
      if (version === reloadVersion.current) setLoading(false)
    }
  }, [])
  const loadedOnce = useRef(false)
  useEffect(() => {
    if (!desktop || !active || loadedOnce.current) return
    loadedOnce.current = true
    void reload().catch(() => {})
  }, [active, reload])
  useEffect(() => {
    localStorage.setItem('bangumi.resource-filters', JSON.stringify(filters))
    setPage(1)
  }, [filters])
  useEffect(() => {
    if (!desktop) return
    let disposed = false
    let unlisten: (() => void) | undefined
    void listen<{ done: number; total: number; path: string; probe: Probe }>(
      'resource-probe-progress',
      (e) => {
        setProgress((p) => ({
          done: Math.max(p.done, e.payload.done),
          total: e.payload.total,
        }))
        setFiles((files) =>
          files.map((f) =>
            f.path === e.payload.path ? { ...f, probe: e.payload.probe } : f,
          ),
        )
      },
    )
      .then((fn) => {
        if (disposed) fn()
        else unlisten = fn
      })
      .catch((e) => notify('error', String(e)))
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [notify])
  const filtered = useMemo(
    () =>
      sortResources(filterResources(files, works, filters), sort.by, sort.desc),
    [files, works, filters, sort],
  )
  const filteredBackups = backups.filter(
    (b) => backupTask === 'all' || b.plan_id === backupTask,
  )
  const visibleSelected = filtered.filter((f) => selected.has(f.path))
  const canReplace = visibleSelected.filter(
    (f) => f.asset && f.available && !f.unavailable,
  )
  const change = (key: keyof ResourceFilters, value: string) =>
    setFilters((f) => ({ ...f, [key]: value }))
  const toggle = (path: string, value: boolean) =>
    setSelected((s) => {
      const next = new Set(s)
      if (value) next.add(path)
      else next.delete(path)
      return next
    })
  const startProbe = async (paths: string[], force: boolean) => {
    setProbing(true)
    setProgress({ done: 0, total: paths.length })
    try {
      await call('probe_resources', { paths, force })
      await reload()
    } catch (e) {
      notify('error', String(e))
    } finally {
      setProbing(false)
    }
  }
  const sortable = (by: ResourceSort, text: string) => (
    <TableHead
      data-column={by}
      aria-sort={
        sort.by === by ? (sort.desc ? 'descending' : 'ascending') : 'none'
      }
    >
      <Button
        variant="ghost"
        size="sm"
        className="resource-sort"
        onClick={() => {
          setSort((s) => ({ by, desc: s.by === by ? !s.desc : true }))
          setPage(1)
        }}
      >
        {text}
        {sort.by === by &&
          (sort.desc ? (
            <ArrowDownIcon aria-hidden="true" />
          ) : (
            <ArrowUpIcon aria-hidden="true" />
          ))}
      </Button>
    </TableHead>
  )
  const eligibleBackups = filteredBackups.filter((b) =>
    ['available', 'error'].includes(b.state),
  )
  const safePage = Math.min(page, Math.max(1, Math.ceil(filtered.length / 80)))
  const safeBackupPage = Math.min(
    backupPage,
    Math.max(1, Math.ceil(filteredBackups.length / 80)),
  )
  if (!active) return null
  return (
    <Tabs value={tab} onValueChange={setTab} className="resources-pane">
      {tabsContainer && createPortal(
        <TabsList aria-label={t('resources.tabs.label')} variant="line">
          <TabsTrigger value="media">{t('resources.tabs.media')}</TabsTrigger>
          <TabsTrigger value="backups">{t('resources.tabs.backups')}</TabsTrigger>
        </TabsList>,
        tabsContainer,
      )}
      <div className="resource-toolbar resource-header">
        <Button
          variant="outline"
          disabled={busy || loading}
          onClick={() =>
            void run(t('resources.actions.refresh'), async () => {
              await call('refresh_library')
              await onReload()
              await reload()
            })
          }
        >
          {t('library.scan.action')}
        </Button>
        <Button
          variant="ghost"
          disabled={busy || loading}
          onClick={() => void run(t('resources.actions.refreshList'), reload)}
        >
          {t('common.actions.refreshList')}
        </Button>
      </div>
      {loading && (
        <p role="status" className="muted">
          {t('resources.loading')}
        </p>
      )}
      {loadError && (
        <Alert variant="destructive">
          <AlertDescription>{loadError}</AlertDescription>
        </Alert>
      )}
      <TabsContent value="media" className="resource-content">
        <div className="resource-filter-panel">
          <div className="resource-filters">
            <Field label={t('resources.search.label')}>
              <Input
                placeholder={t('resources.search.placeholder')}
                value={filters.query}
                onChange={(e) => change('query', e.target.value)}
              />
            </Field>
            <Field label={t('resources.filters.extension')}>
              <Input
                placeholder="ts, m2ts, mkv…"
                value={filters.extension}
                onChange={(e) => change('extension', e.target.value)}
              />
            </Field>
            <Field label={t('resources.filters.codec')}>
              <Input
                placeholder="hevc / h264 / av1"
                value={filters.codec}
                onChange={(e) => change('codec', e.target.value)}
              />
            </Field>
            <Field label={t('resources.filters.status')}>
              <SelectField
                value={filters.status || 'all'}
                onValueChange={(v) => change('status', v === 'all' ? '' : v)}
                aria-label={t('resources.filters.status')}
              >
                <SelectOption value="all">{t('resources.filters.allStatuses')}</SelectOption>
                {Object.entries(statusLabels).map(([value, label]) => (
                  <SelectOption key={value} value={value}>
                    {label}
                  </SelectOption>
                ))}
                <SelectOption value="unknown">
                  {t('resources.filters.unknownMedia')}
                </SelectOption>
              </SelectField>
            </Field>
          </div>
          <Disclosure className="resource-advanced">
            <DisclosureTrigger>{t('resources.filters.ranges')}</DisclosureTrigger>
            <div className="resource-ranges">
              {(
                [
                  ['minSize', t('resources.filters.minSize')],
                  ['maxSize', t('resources.filters.maxSize')],
                  ['minHeight', t('resources.filters.minHeight')],
                  ['maxHeight', t('resources.filters.maxHeight')],
                  ['minBitrate', t('resources.filters.minBitrate')],
                  ['maxBitrate', t('resources.filters.maxBitrate')],
                ] as const
              ).map(([key, label]) => (
                <Field key={key} label={label}>
                  <Input
                    type="number"
                    min="0"
                    step="any"
                    value={filters[key]}
                    onChange={(e) => change(key, e.target.value)}
                  />
                </Field>
              ))}
            </div>
          </Disclosure>
          <div className="resource-toolbar resource-filter-summary">
            <Button
              size="sm"
              variant="secondary"
              aria-pressed={filters.extension === 'ts,m2ts'}
              onClick={() =>
                setFilters((f) => ({ ...f, extension: 'ts,m2ts' }))
              }
            >
              TS / M2TS
            </Button>
            <Button
              size="sm"
              variant="secondary"
              aria-pressed={filters.status === 'missing'}
              onClick={() => change('status', 'missing')}
            >
              {t('resources.probe.pending')}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setFilters(defaultFilters)}
            >
              {t('common.filters.reset')}
            </Button>
            <span className="muted resource-summary">
              {t('resources.selection.summary', {
                count: filtered.length,
                size: bytes(
                  filtered.reduce((n, f) => n + f.signature.size, 0),
                ),
                selected: visibleSelected.length,
              })}
            </span>
          </div>
        </div>
        <div className="resource-toolbar resource-actions">
          <Button
            size="sm"
            variant="outline"
            onClick={() => setSelected(new Set(filtered.map((f) => f.path)))}
          >
            {t('common.selection.allFiltered')}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => setSelected(new Set())}
          >
            {t('common.selection.clear')}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={probing || !visibleSelected.length}
            onClick={() =>
              void startProbe(
                visibleSelected.filter((f) => f.available).map((f) => f.path),
                true,
              )
            }
          >
            {t('resources.probe.selected')}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={probing || !filtered.length}
            onClick={() =>
              void startProbe(
                filtered.filter((f) => f.available).map((f) => f.path),
                false,
              )
            }
          >
            {t('resources.probe.allFiltered')}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={probing || !filtered.some((f) => f.probe?.error)}
            onClick={() =>
              void startProbe(
                filtered
                  .filter((f) => f.available && f.probe?.error)
                  .map((f) => f.path),
                true,
              )
            }
          >
            {t('common.actions.retryFailed')}
          </Button>
          <Button
            size="sm"
            className="resource-primary-action"
            disabled={busy || !canReplace.length}
            onClick={() =>
              onReplace({
                paths: canReplace.map((f) => f.path),
                manual: false,
              })
            }
          >
            {t('resources.replace.scan')}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={busy || !canReplace.length}
            onClick={() => onRename(canReplace.map((f) => f.path))}
          >
            {t('resources.rename.selected')}
          </Button>
          {probing && (
            <>
              <span role="status">
                {t('resources.probe.progress', {
                  completed: progress.done,
                  total: progress.total,
                })}
              </span>
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  void call('cancel_resource_probe').catch((e) =>
                    notify('error', String(e)),
                  )
                }
              >
                {t('resources.probe.cancel')}
              </Button>
            </>
          )}
        </div>
        <div
          className="table-wrap resource-table-wrap"
          tabIndex={0}
          role="region"
          aria-label={t('resources.list.label')}
        >
          <Table className="resource-table">
            <TableHeader>
              <TableRow>
                <TableHead data-column="selection">
                  <span className="sr-only">{t('common.selection.label')}</span>
                </TableHead>
                <TableHead data-column="work">{t('resources.columns.work')}</TableHead>
                {sortable('name', t('common.files.name'))}
                {sortable('extension', t('resources.filters.extension'))}
                {sortable('size', t('common.files.size'))}
                {sortable('resolution', t('media.fields.resolution'))}
                {sortable('bitrate', t('media.fields.totalBitrate'))}
                {sortable('codec', t('resources.filters.codec'))}
                {sortable('duration', t('media.fields.duration'))}
                <TableHead data-column="actions">{t('resources.columns.statusAndActions')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filtered.slice((safePage - 1) * 80, safePage * 80).map((f) => {
                const work = works.find((w) => w.id === f.asset?.work_id)
                const p = f.probe?.error ? null : f.probe
                return (
                  <TableRow
                    key={f.path}
                    data-state={selected.has(f.path) ? 'selected' : undefined}
                  >
                    <TableCell>
                      <Checkbox
                        aria-label={t('common.selection.item', { filename: f.name })}
                        checked={selected.has(f.path)}
                        onCheckedChange={(v) => toggle(f.path, v === true)}
                      />
                    </TableCell>
                    <TableCell className="resource-work">
                      {work ? (
                        <Button
                          size="sm"
                          variant="link"
                          className="resource-title-link"
                          title={work.title}
                          onClick={() => onCatalog(work.id)}
                        >
                          {work.title}
                        </Button>
                      ) : (
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => onCatalog()}
                        >
                          {t('common.status.unlinked')}
                        </Button>
                      )}
                      {f.asset && (
                        <small>
                          {episodePosition(
                            f.asset,
                            work?.kind || f.kind,
                            f.asset.end,
                          )}
                        </small>
                      )}
                    </TableCell>
                    <TableCell className="resource-name">
                      <Button
                        variant="link"
                        size="sm"
                        className="resource-title-link"
                        title={f.name}
                        onClick={() => setDetail(f)}
                      >
                        {f.name}
                      </Button>
                      <FilePath path={f.path} truncate />
                    </TableCell>
                    <TableCell>{f.extension}</TableCell>
                    <TableCell className="resource-number">
                      {bytes(f.signature.size)}
                    </TableCell>
                    <TableCell className="resource-number">
                      {p?.width && p.height
                        ? `${p.width}×${p.height}`
                        : t('common.values.unknown')}
                    </TableCell>
                    <TableCell className="resource-number">
                      {p?.bitrate ? (
                        <>
                          {(p.bitrate / 1e6).toFixed(2)} Mbps
                          {p.bitrate_estimated && <small>{t('media.bitrate.estimated')}</small>}
                        </>
                      ) : (
                        t('common.values.unknown')
                      )}
                    </TableCell>
                    <TableCell>
                      {p?.video_codec ||
                        p?.audio_tracks?.[0]?.codec ||
                        t('common.values.unknown')}
                    </TableCell>
                    <TableCell className="resource-number">
                      {p?.duration
                        ? `${Math.floor(p.duration / 60)}:${String(Math.floor(p.duration % 60)).padStart(2, '0')}`
                        : t('common.values.unknown')}
                    </TableCell>
                    <TableCell className="resource-row-actions">
                      <Badge
                        tone={
                          resourceStatus(f) === 'ready'
                            ? 'green'
                            : resourceStatus(f) === 'error'
                              ? 'red'
                              : resourceStatus(f) === 'missing'
                                ? 'gray'
                                : 'amber'
                        }
                      >
                        {statusLabels[resourceStatus(f)]}
                      </Badge>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={
                          busy || !f.asset || !f.available || f.unavailable
                        }
                        onClick={() =>
                          onReplace({ paths: [f.path], manual: true })
                        }
                      >
                        {t('resources.replace.chooseFile')}
                      </Button>
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        </div>
        <Pages page={safePage} count={filtered.length} onChange={setPage} />
        {!loading && !loadError && !filtered.length && (
          <Empty
            title={
              files.length ? t('resources.empty.filtered') : t('resources.empty.index')
            }
          >
            {files.length
              ? t('resources.empty.filteredHint')
              : t('resources.empty.indexHint')}
          </Empty>
        )}
      </TabsContent>
      <TabsContent value="backups" className="resource-content">
        <p className="muted">
          {t(
            'backups.description',
          )}
        </p>
        <div className="resource-toolbar backup-actions">
          <SelectField
            aria-label={t('backups.filters.task')}
            value={backupTask}
            onValueChange={(v) => {
              setBackupTask(v)
              setBackupPage(1)
            }}
          >
            <SelectOption value="all">{t('backups.filters.allTasks')}</SelectOption>
            {[...new Set(backups.map((b) => b.plan_id))].map((id) => (
              <SelectOption key={id} value={id}>
                {t('backups.task.label', { taskId: id })}
              </SelectOption>
            ))}
          </SelectField>
          <Button
            size="sm"
            variant="outline"
            onClick={() =>
              setBackupSelected(new Set(eligibleBackups.map((b) => b.id)))
            }
          >
            {t('backups.selection.allAvailable')}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => setBackupSelected(new Set())}
          >
            {t('common.selection.clear')}
          </Button>
          <Button
            variant="destructive"
            disabled={
              busy || !eligibleBackups.some((b) => backupSelected.has(b.id))
            }
            onClick={() =>
              setCleanup(
                eligibleBackups.filter((b) => backupSelected.has(b.id)),
              )
            }
          >
            {t('backups.cleanup.selected')}
          </Button>
        </div>
        <div
          className="table-wrap resource-table-wrap"
          tabIndex={0}
          role="region"
          aria-label={t('backups.list.label')}
        >
          <Table className="backup-table">
            <TableHeader>
              <TableRow>
                <TableHead>{t('common.selection.label')}</TableHead>
                <TableHead>{t('backups.columns.files')}</TableHead>
                <TableHead>{t('backups.columns.taskAndDate')}</TableHead>
                <TableHead>{t('common.files.size')}</TableHead>
                <TableHead>{t('common.fields.status')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredBackups
                .slice((safeBackupPage - 1) * 80, safeBackupPage * 80)
                .map((b) => (
                  <TableRow
                    key={b.id}
                    data-state={
                      backupSelected.has(b.id) ? 'selected' : undefined
                    }
                  >
                    <TableCell>
                      <Checkbox
                        aria-label={t('backups.cleanup.item', {
                          filename: filename(b.entry.original),
                        })}
                        disabled={!['available', 'error'].includes(b.state)}
                        checked={backupSelected.has(b.id)}
                        onCheckedChange={(value) =>
                          setBackupSelected((s) => {
                            const next = new Set(s)
                            if (value) next.add(b.id)
                            else next.delete(b.id)
                            return next
                          })
                        }
                      />
                    </TableCell>
                    <TableCell className="backup-paths">
                      <strong>{filename(b.entry.original)}</strong>
                      <FilePath
                        path={b.entry.original}
                        label={t('backups.originalPath')}
                        truncate
                      />
                      <FilePath
                        path={b.entry.backup}
                        label={t('resources.replace.backup')}
                        truncate
                      />
                    </TableCell>
                    <TableCell className="backup-task">
                      <small>{b.plan_id}</small>
                      <small>{formatDate(b.created_at)}</small>
                    </TableCell>
                    <TableCell className="resource-number">
                      {bytes(b.signature.size)}
                    </TableCell>
                    <TableCell>
                      <Badge
                        tone={
                          b.state === 'removed'
                            ? 'gray'
                            : b.state === 'available'
                              ? 'green'
                              : b.state === 'error'
                                ? 'red'
                                : 'amber'
                        }
                      >
                        {{
                          available: t('backups.status.available'),
                          protected: t('backups.status.protected'),
                          unavailable: t('backups.status.unverifiable'),
                          error: t('backups.status.failed'),
                          removed: t('backups.status.removed'),
                          deleting: t('backups.status.pending'),
                        }[b.state] || b.state}
                      </Badge>
                      <small className="error-text">
                        {backendMessage(b.error)}
                      </small>
                    </TableCell>
                  </TableRow>
                ))}
            </TableBody>
          </Table>
        </div>
        <Pages
          page={safeBackupPage}
          count={filteredBackups.length}
          onChange={setBackupPage}
        />
        {!loading && !loadError && !filteredBackups.length && (
          <Empty title={t('backups.empty.title')}>
            {t('backups.empty.description')}
          </Empty>
        )}
      </TabsContent>
      <Dialog
        open={!!detail}
        onOpenChange={(v) => {
          if (!v) setDetail(null)
        }}
      >
        <DialogContent className="resource-detail-dialog">
          <DialogHeader>
            <DialogTitle>{detail?.name}</DialogTitle>
            <DialogDescription className="path-text">
              {detail && <FilePath path={detail.path} />}
            </DialogDescription>
          </DialogHeader>
          <div className="resource-dialog-body">
            {detail && (
              <>
                <p>
                  {bytes(detail.signature.size)} ·{' '}
                  {statusLabels[resourceStatus(detail)]}
                </p>
                <MediaDetails
                  probe={
                    files.find((f) => f.path === detail.path)?.probe ||
                    detail.probe
                  }
                />
                <Button
                  disabled={probing || !detail.available}
                  onClick={() => void startProbe([detail.path], true)}
                >
                  {t('media.actions.refreshProbe')}
                </Button>
              </>
            )}
          </div>
        </DialogContent>
      </Dialog>
      <Dialog
        open={!!cleanup}
        onOpenChange={(v) => {
          if (!v && !busy) setCleanup(null)
        }}
      >
        <DialogContent className="resource-detail-dialog">
          <DialogHeader>
            <DialogTitle>{t('backups.cleanup.title')}</DialogTitle>
            <DialogDescription>
              {t('backups.cleanup.warning')}
            </DialogDescription>
          </DialogHeader>
          <p>
            {t('backups.cleanup.summary', {
              count: cleanup?.length,
              size: bytes(
                cleanup?.reduce((n, b) => n + b.signature.size, 0) || 0,
              ),
            })}
          </p>
          <div className="backup-cleanup-list">
            {cleanup?.map((b) => (
              <p className="path-text" key={b.id}>
                <FilePath path={b.entry.backup} />
                {bytes(b.signature.size)}
              </p>
            ))}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => setCleanup(null)}
            >
              {t('common.actions.cancel')}
            </Button>
            <Button
              variant="destructive"
              disabled={busy}
              onClick={() =>
                void run(t('backups.cleanup.action'), async () => {
                  const results = await call<BackupRecord[]>(
                    'cleanup_backups',
                    { ids: cleanup!.map((b) => b.id) },
                  )
                  const errors = results.filter((b) => b.state !== 'removed')
                  setCleanup(null)
                  setBackupSelected(new Set())
                  await reload()
                  notify(
                    errors.length ? 'error' : 'success',
                    errors.length
                      ? t('backups.cleanup.incomplete', {
                          count: errors.length,
                          errors: errors
                            .map((b) => backendMessage(b.error))
                            .join('；'),
                        })
                      : t('backups.cleanup.success'),
                  )
                })
              }
            >
              {t('common.actions.confirmDeletePermanently')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Tabs>
  )
}
