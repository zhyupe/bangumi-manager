import { configureSubtitlePattern } from '@/lib/subtitles'
import {
  ArchiveIcon,
  DashboardIcon,
  FileIcon,
  GearIcon,
} from '@radix-ui/react-icons'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { useCallback, useEffect, useRef, useState } from 'react'
import { FileActionsContext } from '@/components/file-path'
import { Notifications, useNotifications } from '@/components/notifications'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import CatalogPane from '@/features/library/CatalogPane'
import type { LibraryDrop } from '@/features/library/import-model'
import MediaImportDialog, {
  type ImportTarget,
} from '@/features/metadata/MediaImportDialog'
import SourcePane, { type CreateMode } from '@/features/organize/SourcePane'
import RenameDialog from '@/features/resources/RenameDialog'
import ReplacementDialog from '@/features/resources/ReplacementDialog'
import ResourcesPane from '@/features/resources/ResourcesPane'
import SettingsPane from '@/features/settings/SettingsPane'
import JobsPane from '@/features/tasks/JobsPane'
import { t, useLocale } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import { call, desktop, measureAction } from '@/lib/api'
import { empty } from '@/lib/defaults'
import type {
  Mapping,
  Plan,
  QueueState,
  ReplacementTarget,
  Run,
  Snapshot,
  Work,
} from '@/types'
import { version } from '../../package.json'
import '@/styles/theme.css'
import '@/styles/global.css'

const pages = [
  {
    id: 'sources',
    get title() {
      return t('nav.organize')
    },
    icon: FileIcon,
  },
  {
    id: 'catalog',
    get title() {
      return t('nav.library')
    },
    icon: DashboardIcon,
  },
  {
    id: 'resources',
    get title() {
      return t('nav.resources')
    },
    icon: FileIcon,
  },
  {
    id: 'jobs',
    get title() {
      return t('nav.tasks')
    },
    icon: ArchiveIcon,
  },
  {
    id: 'settings',
    get title() {
      return t('nav.settings')
    },
    icon: GearIcon,
  },
]
export default function App() {
  useLocale()
  const [data, setData] = useState<Snapshot>(empty),
    [page, setPage] = useState('sources'),
    [catalogKind, setCatalogKind] = useState('tv'),
    [workId, setWorkId] = useState(''),
    [planId, setPlanId] = useState('')
  const [catalogRequest, setCatalogRequest] = useState('')
  const [resourceTabsContainer, setResourceTabsContainer] = useState<HTMLDivElement | null>(null)
  const [libraryDrop, setLibraryDrop] = useState<LibraryDrop | null>(null)
  const dropSequence = useRef(0)
  const [replacementTarget, setReplacementTarget] =
    useState<ReplacementTarget | null>(null)
  const [renamePaths, setRenamePaths] = useState<string[] | null>(null)
  const [importTarget, setImportTarget] = useState<ImportTarget | null>(null)
  const [busy, setBusy] = useState(''),
    [dragging, setDragging] = useState(false),
    [progress, setProgress] = useState('')
  const [mappings, setMappings] = useState<Record<string, Mapping>>(() => {
    try {
      return JSON.parse(localStorage.getItem('bangumi.mappings') || '{}')
    } catch {
      return {}
    }
  })
  const { items: notifications, notify, dismiss } = useNotifications()
  const setMessage = useCallback(
    (text: string) => {
      if (text) notify('error', text)
    },
    [notify],
  )
  const setNotice = useCallback(
    (text: string) => {
      if (text) notify('success', text)
    },
    [notify],
  )
  const running = data.plans.some(
    (p) => p.state === 'running' || p.state === 'cancelling',
  )
  const queue = data.queue || { ids: [], paused: false, error: '' }
  useEffect(() => {
    if (!desktop)
      notify(
        'warning',
        t('app.preview.banner'),
        undefined,
        'preview',
      )
  }, [notify])
  useEffect(() => {
    if (!data.data_dir) return
    if (
      ![
        data.settings.tv_root,
        data.settings.movie_root,
        data.settings.output_root,
        data.settings.music_root,
      ].some(Boolean)
    ) {
      notify(
        'warning',
        t('app.library.notConfigured'),
        { label: t('app.library.configure'), run: () => setPage('settings') },
        'setup',
      )
    } else dismiss('setup')
  }, [data.settings, data.data_dir, notify, dismiss])
  useEffect(
    () => localStorage.setItem('bangumi.mappings', JSON.stringify(mappings)),
    [mappings],
  )
  const deletedPlans = useRef(new Set<string>())
  const deletedWorks = useRef(new Set<string>())
  const planUpdates = useRef(new Map<string, number>())
  const queueUpdates = useRef(0)
  const reloadSequence = useRef(0)
  const reload = useCallback(async () => {
    const sequence = ++reloadSequence.current
    const versions = new Map(planUpdates.current)
    const queueVersion = queueUpdates.current
    const s = await call<Snapshot>('snapshot')
    if (sequence !== reloadSequence.current) return
    configureSubtitlePattern(s.settings.subtitle_language_pattern)
    setData((current) => {
      const newer = current.plans.filter(
        (p) => planUpdates.current.get(p.id) !== versions.get(p.id),
      )
      return {
        ...s,
        works: s.works.filter((w) => !deletedWorks.current.has(w.id)),
        queue: queueUpdates.current !== queueVersion ? current.queue : s.queue,
        plans: [
          ...s.plans.filter((p) => !newer.some((n) => n.id === p.id)),
          ...newer,
        ].filter((p) => !deletedPlans.current.has(p.id)),
      }
    })
  }, [])
  const run: Run = useCallback(
    async (label, fn) => {
      setBusy(label)
      try {
        await measureAction(label, fn)
      } catch (e) {
        setMessage(e instanceof Error ? e.message : String(e))
      } finally {
        setBusy('')
        setProgress('')
      }
    },
    [setMessage],
  )
  useEffect(() => {
    if (desktop) void run(t('app.library.read'), reload)
  }, [reload, run])
  const updatePlan = useCallback((p: Plan) => {
    planUpdates.current.set(p.id, (planUpdates.current.get(p.id) || 0) + 1)
    setData((d) => {
      if (deletedPlans.current.has(p.id)) return d
      const previous = d.plans.find((x) => x.id === p.id)
      // IPC replies and progress events can arrive in a different order.
      if (
        (['complete', 'cancelled'].includes(previous?.state || '') &&
          p.state !== previous?.state) ||
        (previous?.state === 'cancelling' &&
          ['draft', 'running', 'paused'].includes(p.state))
      )
        return d
      return { ...d, plans: [...d.plans.filter((x) => x.id !== p.id), p] }
    })
  }, [])
  const deletePlans = (ids: string[]) => {
    const removed = new Set(ids)
    ids.forEach((id) => deletedPlans.current.add(id))
    setData((d) => ({
      ...d,
      plans: d.plans.filter((p) => !removed.has(p.id)),
    }))
    setPlanId((current) =>
      removed.has(current)
        ? [...data.plans]
            .filter((p) => !removed.has(p.id))
            .sort((a, b) => b.created_at.localeCompare(a.created_at))[0]?.id ||
          ''
        : current,
    )
    setNotice(
      ids.length === 1
        ? t('tasks.notifications.deleted')
        : t('tasks.notifications.cleared', { count: ids.length }),
    )
  }
  const deleteWork = async (id: string) => {
    await call('delete_work', { workId: id })
    deletedWorks.current.add(id)
    setData((current) => ({
      ...current,
      works: current.works.filter((w) => w.id !== id),
    }))
    setMappings((current) =>
      Object.fromEntries(
        Object.entries(current).filter(([, mapping]) => mapping.work_id !== id),
      ),
    )
    setWorkId((current) => (current === id ? '' : current))
    setCatalogRequest((current) => (current === id ? '' : current))
    setLibraryDrop((current) => (current?.workId === id ? null : current))
    setNotice(t('library.delete.success'))
  }
  const showPlan = (p: Plan, view = true) => {
    updatePlan(p)
    setPlanId(p.id)
    if (view) setPage('jobs')
    else
      setNotice(
        t('tasks.notifications.created', {
          count: p.rows.filter((r) => !r.parent_id).length,
        }),
      )
  }
  const created = async (plan: Plan, mode: CreateMode) => {
    if (mode !== 'execute') {
      showPlan(plan, mode === 'view')
      return
    }
    updatePlan(plan)
    setPlanId(plan.id)
    try {
      await call<Plan>('run_plan', { id: plan.id })
      notify(
        queue.paused ? 'warning' : 'success',
        queue.paused
          ? t('tasks.notifications.queuedPaused')
          : t('tasks.notifications.queued'),
        {
          label: t('tasks.actions.view'),
          run: () => {
            setPlanId(plan.id)
            setPage('jobs')
          },
        },
      )
    } catch (error) {
      notify(
        'error',
        t('tasks.notifications.queueFailed', { error: String(error) }),
        {
          label: t('tasks.actions.inspect'),
          run: () => {
            setPlanId(plan.id)
            setPage('jobs')
          },
        },
      )
    }
  }
  const scanPaths = async (paths: string[]) => {
    await run(t('organize.scan.action'), async () => {
      const result = await call<{ errors: string[] }>('scan_files', { paths })
      await reload()
      if (result.errors.length) notify('warning', result.errors.join('\n'))
      else setNotice(t('organize.scan.complete'))
    })
  }
  const onDrop = useRef<(paths: string[], target: Element | null) => void>(
    () => {},
  )
  onDrop.current = (paths, target) => {
    if (busy) {
      notify('warning', t('organize.drop.busy'))
      return
    }
    const row = target?.closest<HTMLElement>(
      '[data-drop-episode], [data-drop-extras]',
    )
    if (row) {
      if (row.dataset.dropDisabled === 'true') {
        notify(
          'warning',
          row.hasAttribute('data-drop-extras')
            ? t('organize.drop.busy')
            : t('library.import.episodeDisabled'),
        )
        return
      }
      setLibraryDrop({
        id: ++dropSequence.current,
        paths,
        workId: row.dataset.dropWork!,
        episodeId: row.dataset.dropEpisode,
      })
    } else {
      setPage('sources')
      void scanPaths(paths)
    }
  }
  useEffect(() => {
    if (!desktop) return
    let disposed = false
    const cleanups: (() => void)[] = []
    const retain = (p: Promise<() => void>) => {
      void p
        .then((fn) => (disposed ? fn() : cleanups.push(fn)))
        .catch((e) => setMessage(String(e)))
    }
    retain(
      listen<number>('scan-progress', (e) =>
        setProgress(t('organize.scan.fileCount', { count: e.payload })),
      ),
    )
    retain(
      listen<{ done: number; total: number }>('plan-progress', (e) =>
        setProgress(
          t('tasks.creation.progress', {
            completed: e.payload.done,
            total: e.payload.total,
          }),
        ),
      ),
    )
    retain(listen<Plan>('job-progress', (e) => updatePlan(e.payload)))
    retain(
      listen<Plan>('job-finished', (e) => {
        updatePlan(e.payload)
        const plan = e.payload
        const name = plan.works.map((w) => w.title).join('、')
        const action = {
          label: t('tasks.actions.view'),
          run: () => {
            setPlanId(plan.id)
            setPage('jobs')
          },
        }
        if (plan.state === 'complete') {
          const count =
            (plan.metadata_notices?.length || 0) +
            (plan.cleanup_notices?.length || 0)
          notify(
            count ? 'warning' : 'success',
            t('tasks.notifications.completed', {
              task: name,
              notices: count ? t('tasks.notifications.noticeCount', { count: count }) : '',
            }),
            action,
            `job-${plan.id}`,
          )
        } else if (plan.state === 'cancelled' || plan.state === 'paused') {
          notify(
            'warning',
            `${name}：${plan.state === 'cancelled' ? t('tasks.notifications.cancelled') : t('tasks.notifications.queuePaused')}`,
            action,
            `job-${plan.id}`,
          )
        } else {
          const error =
            plan.rows.find((r) => r.error)?.error ||
            plan.metadata_errors[0] ||
            t('tasks.notifications.executionError')
          notify(
            'error',
            t('tasks.notifications.failed', {
              task: name,
              error: backendMessage(error),
            }),
            action,
            `job-${plan.id}`,
          )
        }
        void reload().catch((e) => setMessage(String(e)))
      }),
    )
    retain(
      listen<string>('job-error', (e) => {
        setMessage(e.payload)
        void reload().catch(() => {})
      }),
    )
    retain(
      listen<QueueState>('queue-changed', (e) => {
        queueUpdates.current += 1
        setData((data) => ({ ...data, queue: e.payload }))
      }),
    )
    retain(
      getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload
        if (p.type === 'enter') {
          setDragging(true)
        } else if (p.type === 'over') {
          document
            .querySelectorAll('.drop-active')
            .forEach((e) => e.classList.remove('drop-active'))
          document
            .elementFromPoint(
              p.position.x / window.devicePixelRatio,
              p.position.y / window.devicePixelRatio,
            )
            ?.closest('[data-drop-episode], [data-drop-extras]')
            ?.classList.add('drop-active')
        } else if (p.type === 'leave') {
          setDragging(false)
          document
            .querySelectorAll('.drop-active')
            .forEach((e) => e.classList.remove('drop-active'))
        } else if (p.type === 'drop') {
          setDragging(false)
          document
            .querySelectorAll('.drop-active')
            .forEach((e) => e.classList.remove('drop-active'))
          onDrop.current(
            p.paths,
            document.elementFromPoint(
              p.position.x / window.devicePixelRatio,
              p.position.y / window.devicePixelRatio,
            ),
          )
        }
      }),
    )
    return () => {
      disposed = true
      cleanups.forEach((fn) => fn())
    }
  }, [reload, updatePlan, notify, setMessage])
  const blocked = Boolean(busy) || running
  const pendingPlans = data.plans.filter(
    (p) => !['complete', 'cancelled'].includes(p.state),
  ).length
  const bound = async (w: Work) => {
    await reload()
    setWorkId(w.id)
    setNotice(t('library.notifications.linked', { title: w.title }))
  }
  return (
    <FileActionsContext.Provider value={notify}>
      <div className="app-shell">
        <aside className="sidebar">
          <nav aria-label={t('nav.label')}>
            {pages.map((item) => (
              <Button
                key={item.id}
                className={page === item.id ? 'active' : ''}
                onClick={() => setPage(item.id)}
                variant="ghost"
                aria-current={page === item.id ? 'page' : undefined}
                aria-label={item.title}
                title={
                  item.id === 'jobs' && pendingPlans > 0
                    ? t('nav.tasks.unfinishedCount', { count: pendingPlans })
                    : undefined
                }
              >
                <item.icon />
                <span>{item.title}</span>
                {item.id === 'jobs' && pendingPlans > 0 && (
                  <Badge className="nav-task-badge" aria-hidden="true">
                    {pendingPlans > 99 ? '99+' : pendingPlans}
                  </Badge>
                )}
              </Button>
            ))}
          </nav>
          <div
            className="sidebar-version"
            aria-label={t('app.version.label', { version: version })}
          >
            v{version}
          </div>
        </aside>
        <div
          className={`workspace${page === 'sources' ? ' sources-workspace' : ''}`}
        >
          <header className="topbar">
            <div className="topbar-navigation">
              <h1>{pages.find((p) => p.id === page)?.title}</h1>
              {page === 'catalog' && (
                <Tabs
                  value={catalogKind}
                  onValueChange={setCatalogKind}
                  className="catalog-tabs"
                >
                  <TabsList aria-label={t('library.kind.label')} variant="line">
                    <TabsTrigger value="movie">{t('common.media.movie')}</TabsTrigger>
                    <TabsTrigger value="tv">{t('common.media.tv')}</TabsTrigger>
                    <TabsTrigger value="music">{t('common.media.music')}</TabsTrigger>
                  </TabsList>
                </Tabs>
              )}
              {page === 'resources' && <div className="catalog-tabs group/tabs" data-orientation="horizontal" ref={setResourceTabsContainer} />}
            </div>
            <div className="topbar-meta">
              <span>{t('library.summary.workCount', { count: data.works.length })}</span>
              <span>
                {t('library.summary.resourceCount', { count: data.index.assets.length })}
              </span>
            </div>
          </header>
          <main className={page === 'sources' ? 'source-page' : undefined}>
            {(page === 'sources' || page === 'catalog') && (
              <div className="main-content">
                {page === 'sources' ? (
                  <SourcePane
                    defaultOperation={data.settings.default_operation || 'copy'}
                    cueSourceDisposition={data.settings.cue_source_disposition || 'keep'}
                    files={data.sources}
                    works={data.works}
                    workId={workId}
                    onWork={setWorkId}
                    run={run}
                    busy={Boolean(busy)}
                    onScan={scanPaths}
                    onReload={reload}
                    onAdd={setImportTarget}
                    onPlan={created}
                    mappings={mappings}
                    onMappings={setMappings}
                  />
                ) : (
                  <CatalogPane
                    plans={data.plans}
                    onDelete={deleteWork}
                    detailRequest={catalogRequest}
                    onDetailOpened={() => setCatalogRequest('')}
                    kind={catalogKind}
                    works={data.works}
                    workId={workId}
                    onWork={setWorkId}
                    index={data.index}
                    busy={blocked}
                    importBusy={Boolean(busy)}
                    run={run}
                    onReload={reload}
                    onPlan={created}
                    dropRequest={libraryDrop}
                    onDropHandled={() => setLibraryDrop(null)}
                    onReplace={setReplacementTarget}
                    onRename={setRenamePaths}
                    onAdd={setImportTarget}
                  />
                )}
              </div>
            )}
            <div hidden={page !== 'resources'}>
              <ResourcesPane
                active={page === 'resources'}
                works={data.works}
                tabsContainer={resourceTabsContainer}
                run={run}
                busy={blocked}
                onReload={reload}
                onReplace={setReplacementTarget}
                onRename={setRenamePaths}
                notify={notify}
                onCatalog={(id) => {
                  if (id) {
                    setCatalogRequest(id)
                    setWorkId(id)
                    setCatalogKind(
                      data.works.find((w) => w.id === id)?.kind || 'tv',
                    )
                  }
                  setPage('catalog')
                }}
              />
            </div>
            {page === 'settings' && (
              <SettingsPane
                settings={data.settings}
                dataDir={data.data_dir}
                run={run}
                busy={blocked}
                onSaved={async () => {
                  await reload()
                  setNotice(t('settings.notifications.saved'))
                }}
              />
            )}
            {page === 'jobs' && (
              <JobsPane
                plans={data.plans}
                planId={planId}
                onPlanId={setPlanId}
                run={run}
                busy={Boolean(busy)}
                running={running}
                onDelete={deletePlans}
                onUpdate={updatePlan}
                queue={queue}
                notify={notify}
              />
            )}
          </main>
        </div>
        <ReplacementDialog
          target={replacementTarget}
          onClose={() => setReplacementTarget(null)}
          onPlan={showPlan}
          run={run}
          busy={Boolean(busy)}
        />
        <RenameDialog
          paths={renamePaths}
          onClose={() => setRenamePaths(null)}
          onPlan={showPlan}
          run={run}
          busy={Boolean(busy)}
        />
        <MediaImportDialog
          target={importTarget}
          works={data.works}
          busy={Boolean(busy)}
          mutationBusy={running}
          run={run}
          onClose={() => setImportTarget(null)}
          onBound={bound}
        />
        <Notifications
          items={notifications}
          dismiss={dismiss}
          activity={busy || (running ? t('common.status.processing') : '')}
          progress={progress}
        />
        {dragging && (
          <div className="drop-overlay">
            <div>
              {t('library.import.dropHint')}
            </div>
          </div>
        )}
      </div>
    </FileActionsContext.Provider>
  )
}
