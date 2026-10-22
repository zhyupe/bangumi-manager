import {
  Cross2Icon,
  DownloadIcon,
  PauseIcon,
  PlayIcon,
  TrashIcon,
} from '@radix-ui/react-icons'
import { save } from '@tauri-apps/plugin-dialog'
import { useEffect, useState } from 'react'
import { Disclosure, DisclosureTrigger } from '@/components/disclosure'
import { Empty } from '@/components/empty-state'
import { FilePath } from '@/components/file-path'
import type { Notify } from '@/components/notifications'
import { Pages } from '@/components/pagination'
import { ProbeInfo } from '@/components/probe-info'
import { SelectField, SelectOption } from '@/components/select-field'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { formatDate, t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import { call } from '@/lib/api'
import { bytes, filename, natural } from '@/lib/format'
import type { Plan, PlanRow, QueueState, Run } from '@/types'
import { Status } from './TaskStatus'

const operations = {
  get copy() {
    return t('common.operations.copy')
  },
  get move() {
    return t('common.operations.move')
  },
  get hardlink() {
    return t('common.operations.hardlink')
  },
  get symlink() {
    return t('common.operations.symlink')
  },
  get adopt() {
    return t('common.operations.adopt')
  },
  get rename() {
    return t('common.operations.rename')
  },
  get split() {
    return t('common.operations.cueSplit')
  },
} as const
const actions = {
  get undecided() {
    return t('tasks.decisions.pending')
  },
  get copy() {
    return t('tasks.decisions.keep')
  },
  get replace() {
    return t('tasks.decisions.replace')
  },
  get skip() {
    return t('tasks.decisions.skip')
  },
}
export default function JobsPane({
  plans,
  planId,
  onPlanId,
  run,
  busy,
  running,
  onUpdate,
  onDelete,
  queue,
  notify,
}: {
  plans: Plan[]
  planId: string
  onPlanId: (id: string) => void
  run: Run
  busy: boolean
  running: boolean
  onUpdate: (p: Plan) => void
  onDelete: (ids: string[]) => void
  queue: QueueState
  notify: Notify
}) {
  const current = plans.find((p) => p.id === planId)
  const [draft, setDraft] = useState(current),
    [page, setPage] = useState(1),
    [confirm, setConfirm] = useState(false),
    [onlyConflicts, setOnlyConflicts] = useState(false)
  const [deleteIds, setDeleteIds] = useState<string[]>([])
  const deleting = plans.filter((p) => deleteIds.includes(p.id))
  const finished = plans.filter((p) =>
    ['complete', 'cancelled'].includes(p.state),
  )
  const [batchOperation, setBatchOperation] =
    useState<NonNullable<PlanRow['operation']>>('copy')
  const [batchAction, setBatchAction] = useState('copy')
  useEffect(() => {
    setDraft(current)
  }, [current])
  useEffect(() => {
    setDeleteIds([])
    setConfirm(false)
  }, [planId])
  useEffect(() => setPage(1), [planId, onlyConflicts])
  const rows = [...(draft?.rows || [])]
    .filter(
      (r) =>
        !onlyConflicts || r.conflicts.length || r.warnings.length || r.error,
    )
    .sort((a, b) => natural(a.source, b.source))
  const completed =
    draft?.rows.filter((r) => r.state === 'complete').length || 0
  const renaming = draft?.kind === 'rename'
  const renameStarted =
    renaming &&
    (draft?.state === 'running' ||
      draft?.rows.some((r) => r.rename && r.rename.phase !== 'pending'))
  const editable =
    !!draft &&
    !renaming &&
    draft?.kind !== 'cue_split' &&
    !['queued', 'running', 'cancelling', 'cancelled', 'complete'].includes(
      draft.state,
    ) &&
    !completed &&
    !draft?.rows.some((r) => !!r.digest || r.backups_prepared || !!r.remote)
  const stopping = draft?.state === 'cancelling'
  const terminal = draft?.state === 'cancelled' || draft?.state === 'complete'
  const changeRows = (ids: Set<string>, changes: Partial<PlanRow>) => {
    setDraft(
      (prev) =>
        prev && {
          ...prev,
          rows: prev.rows.map((r) =>
            ids.has(r.id) || (!!r.parent_id && ids.has(r.parent_id))
              ? { ...r, ...changes }
              : r,
          ),
        },
    )
  }
  const changeAllRows = (changes: Partial<PlanRow>) => {
    setDraft(
      (prev) =>
        prev && {
          ...prev,
          rows: prev.rows.map((row) => ({ ...row, ...changes })),
        },
    )
  }
  const start = () =>
    run(t('tasks.actions.start'), async () => {
      if (!draft) return
      if (editable) {
        const p = await call<Plan>('save_decisions', {
          id: draft.id,
          choices: draft.rows
            .filter((r) => !r.parent_id)
            .map((r) => ({
              id: r.id,
              action: r.action,
              operation: r.operation || 'copy',
              reviewed: r.reviewed,
            })),
        })
        onUpdate(p)
      }
      await call<Plan>('run_plan', { id: draft.id })
      notify(
        queue.paused ? 'warning' : 'success',
        queue.paused
          ? t('tasks.notifications.queuedPaused')
          : t('tasks.notifications.queued'),
      )
      setConfirm(false)
    })
  return (
    <div className="jobs-layout">
      <Card className="panel job-list">
        <div className="job-list-header">
          <h2>{t('tasks.title')}</h2>
          <p className="queue-summary">
            {t('tasks.queue.waitingCount', {
              status: queue.paused
                ? t('tasks.queue.paused')
                : running
                  ? t('tasks.queue.running')
                  : t('tasks.queue.idle'),
              count: plans.filter((p) => p.state === 'queued').length,
            })}
          </p>
          {queue.paused && queue.ids.length > 0 && (
            <Button
              disabled={busy || running}
              onClick={() =>
                void run(t('tasks.queue.resume'), async () => {
                  await call('resume_queue')
                  notify('success', t('tasks.queue.resumed'))
                })
              }
            >
              <PlayIcon />
              {t('tasks.queue.resume')}
            </Button>
          )}
          <Button
            size="sm"
            variant="secondary"
            disabled={busy || running || !finished.length}
            title={t('tasks.cleanup.selection', {
              count: finished.length,
            })}
            onClick={() => setDeleteIds(finished.map((p) => p.id))}
          >
            <TrashIcon />
            {t('tasks.cleanup.action')}
          </Button>
        </div>
        {[...plans]
          .sort((a, b) => b.created_at.localeCompare(a.created_at))
          .map((p) => (
            <Button
              className={`job-item ${planId === p.id ? 'selected' : ''}`}
              key={p.id}
              onClick={() => onPlanId(p.id)}
              variant="ghost"
            >
              <span>{p.works.map((w) => w.title).join('、')}</span>
              <small>
                {t('tasks.summary.fileCount', {
                  date: formatDate(p.created_at),
                  count: p.rows.length,
                })}
              </small>
              <Status state={p.state} />
            </Button>
          ))}
      </Card>
      <Card className="panel job-detail">
        {draft ? (
          <>
            <div className="section-title">
              <div>
                <h2>{renaming ? t('tasks.review.renameTitle') : t('tasks.review.organizeTitle')}</h2>
              </div>
              <Status state={draft.state} />
            </div>
            <div className="plan-summary">
              <div>
                <strong>
                  {draft.rows.filter((r) => !r.attachment).length}
                </strong>
                <span>{t('tasks.summary.mediaAndExtras')}</span>
              </div>
              <div>
                <strong>{draft.rows.filter((r) => r.attachment).length}</strong>
                <span>{renaming ? t('tasks.summary.attachments') : t('common.files.subtitle')}</span>
              </div>
              <div>
                <strong>
                  {bytes(
                    draft.rows
                      .filter((r) => r.action !== 'skip')
                      .reduce((n, r) => n + r.signature.size, 0),
                  )}
                </strong>
                <span>{t('tasks.summary.pendingSize')}</span>
              </div>
              <div>
                <strong>{completed}</strong>
                <span>{t('tasks.summary.completed')}</span>
              </div>
            </div>
            <div className="selection-bar">
              <Label className="check-label">
                <Checkbox
                  checked={onlyConflicts}
                  onCheckedChange={(v) => setOnlyConflicts(v === true)}
                />
                {t('tasks.filters.onlyIssues')}
              </Label>
              <div className="button-row">
                <Button
                  variant="secondary"
                  disabled={busy || draft.state === 'running' || stopping}
                  onClick={() =>
                    void run(t('tasks.export.action'), async () => {
                      let p = draft
                      if (editable) {
                        p = await call<Plan>('save_decisions', {
                          id: draft.id,
                          choices: draft.rows
                            .filter((r) => !r.parent_id)
                            .map((r) => ({
                              id: r.id,
                              action: r.action,
                              operation: r.operation || 'copy',
                              reviewed: r.reviewed,
                            })),
                        })
                        onUpdate(p)
                      }
                      const path = await save({
                        defaultPath: `bangumi-plan-${draft.id.slice(0, 8)}.json`,
                        filters: [{ name: 'JSON', extensions: ['json'] }],
                      })
                      if (path) await call('export_plan', { id: p.id, path })
                    })
                  }
                >
                  <DownloadIcon />
                  {t('tasks.export.label')}
                </Button>
                {draft.state === 'running' ? (
                  <Button
                    data-tone="amber"
                    disabled={busy}
                    onClick={() =>
                      void run(t('tasks.actions.pause'), async () => {
                        await call('cancel_job', { id: draft.id })
                        notify('warning', t('tasks.notifications.pausing'))
                      })
                    }
                  >
                    <PauseIcon />
                    {t('common.actions.pause')}
                  </Button>
                ) : !terminal && !stopping && draft.state !== 'queued' ? (
                  <Button disabled={busy} onClick={() => setConfirm(true)}>
                    <PlayIcon />
                    {draft.state === 'draft'
                      ? t('tasks.confirm.title')
                      : draft.state === 'metadata_failed'
                        ? t('tasks.actions.retryMetadata')
                        : t('tasks.actions.resume')}
                  </Button>
                ) : null}
                {terminal && (
                  <Button
                    variant="outline"
                    data-tone="red"
                    disabled={busy || running}
                    onClick={() => setDeleteIds([draft.id])}
                  >
                    <TrashIcon />
                    {t('common.actions.deletePermanently')}
                  </Button>
                )}
                {!terminal && (
                  <Button
                    variant="outline"
                    disabled={busy || stopping || renameStarted}
                    title={
                      renameStarted
                        ? t('tasks.cancel.renameStarted')
                        : undefined
                    }
                    onClick={() =>
                      void run(t('tasks.actions.cancel'), async () => {
                        const p = await call<Plan>('cancel_plan', {
                          id: draft.id,
                        })
                        onUpdate(p)
                        notify(
                          'warning',
                          p.state === 'cancelling'
                            ? t('tasks.cancel.inProgress')
                            : t('tasks.cancel.completed'),
                        )
                        setConfirm(false)
                      })
                    }
                  >
                    <Cross2Icon />
                    {stopping ? t('tasks.status.cancelling') : t('tasks.actions.cancel')}
                  </Button>
                )}
              </div>
            </div>
            {editable && draft.kind !== 'replacement' && (
              <div className="job-batch-controls">
                <small>
                  {t(
                    'tasks.bulk.scope',
                    { count: draft.rows.length },
                  )}
                </small>
                <div className="job-batch-row">
                  <SelectField
                    aria-label={t('tasks.bulk.operation')}
                    value={batchOperation}
                    onValueChange={(value) =>
                      setBatchOperation(value as typeof batchOperation)
                    }
                  >
                    {Object.entries(operations)
                      .filter(
                        ([value]) => value !== 'adopt' && value !== 'rename',
                      )
                      .map(([value, label]) => (
                        <SelectOption key={value} value={value}>
                          {label}
                        </SelectOption>
                      ))}
                  </SelectField>
                  <Button
                    variant="secondary"
                    disabled={busy || !draft.rows.length}
                    onClick={() => changeAllRows({ operation: batchOperation })}
                  >
                    {t('tasks.bulk.applyOperation')}
                  </Button>
                  <SelectField
                    aria-label={t('tasks.bulk.decision')}
                    value={batchAction}
                    onValueChange={(value) => setBatchAction(value)}
                  >
                    {Object.entries(actions).map(([value, label]) => (
                      <SelectOption key={value} value={value}>
                        {label}
                      </SelectOption>
                    ))}
                  </SelectField>
                  <Button
                    variant="secondary"
                    disabled={busy || !draft.rows.length}
                    onClick={() => changeAllRows({ action: batchAction })}
                  >
                    {t('tasks.bulk.applyDecision')}
                  </Button>
                </div>
                <small>
                  {t(
                    'tasks.bulk.operationHint',
                  )}
                </small>
              </div>
            )}
            {draft.metadata_errors.length > 0 && (
              <Disclosure defaultOpen className="error-details">
                <DisclosureTrigger>{t('tasks.metadata.retryRequired')}</DisclosureTrigger>
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
            {!!draft.cleanup_notices?.length && (
              <Disclosure>
                <DisclosureTrigger>
                  {t('tasks.cleanup.noticeCount', {
                    count: draft.cleanup_notices.length,
                  })}
                </DisclosureTrigger>
                {draft.cleanup_notices.map((notice, i) => (
                  <p className="path-text" key={i}>
                    {backendMessage(notice)}
                  </p>
                ))}
              </Disclosure>
            )}
            <div className="table-wrap">
              <Table className="plan-table">
                <TableHeader>
                  <TableRow>
                    <TableHead className="plan-path-col">
                      {t('tasks.columns.paths')}
                    </TableHead>
                    <TableHead>{t('media.details.title')}</TableHead>
                    <TableHead>{t('tasks.columns.actionAndStatus')}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.slice((page - 1) * 80, page * 80).map((row) => (
                    <TableRow
                      key={row.id}
                      className={row.action === 'skip' ? 'skipped-row' : ''}
                    >
                      <TableCell>
                        <div className="file-name">
                          {filename(row.source)}{' '}
                          {!!row.part && (
                            <Badge tone="gray">Part {row.part}</Badge>
                          )}
                          {row.attachment && (
                            <Badge tone="gray">{t('common.files.subtitle')}</Badge>
                          )}
                          {row.extra && (
                            <Badge tone="gray">{t('common.files.extra')}</Badge>
                          )}
                        </div>
                        <FilePath path={row.source} />
                        <div className="target-path">
                          <FilePath
                            path={row.target}
                            label="→ "
                            previousPaths={[row.source]}
                          />
                        </div>
                        {row.subtitle && (
                          <FilePath
                            path={row.subtitle.asset.path}
                            label={t('organize.subtitles.targetVideo')}
                          />
                        )}
                        {row.conflicts.map((c) => (
                          <p className="conflict" key={c}>
                            {c}
                          </p>
                        ))}
                        {row.error && (
                          <p className="error-text">
                            {backendMessage(row.error)}
                          </p>
                        )}
                        {row.backups.length > 0 && (
                          <Disclosure>
                            <DisclosureTrigger>
                              {t('tasks.columns.backups')}
                            </DisclosureTrigger>
                            {row.backups.map((b, i) => (
                              <p className="path-text" key={b.original}>
                                <FilePath
                                  path={b.original}
                                  previousPaths={[
                                    row.source,
                                    row.target,
                                    ...row.backups
                                      .slice(0, i)
                                      .flatMap((previous) => [
                                        previous.original,
                                        previous.backup,
                                      ]),
                                  ]}
                                />
                                {bytes(b.signature.size)}
                                <ProbeInfo probe={b.probe || null} />
                                {b.backup ? (
                                  <FilePath
                                    path={b.backup}
                                    label="→ "
                                    previousPaths={[
                                      row.source,
                                      row.target,
                                      b.original,
                                      ...row.backups
                                        .slice(0, i)
                                        .flatMap((previous) => [
                                          previous.original,
                                          previous.backup,
                                        ]),
                                    ]}
                                  />
                                ) : (
                                  <span>{t('tasks.backup.generatedAtExecution')}</span>
                                )}
                              </p>
                            ))}
                          </Disclosure>
                        )}
                      </TableCell>
                      <TableCell>
                        <strong>{bytes(row.signature.size)}</strong>
                        {!row.subtitle && <ProbeInfo probe={row.probe} />}
                        {row.target_signature && (
                          <div className="existing-version">
                            <small>
                              {t('tasks.target.existingSize', {
                                size: bytes(row.target_signature.size),
                              })}
                            </small>
                            {!row.subtitle && (
                              <ProbeInfo probe={row.target_probe} />
                            )}
                          </div>
                        )}
                      </TableCell>
                      <TableCell>
                        {!row.parent_id &&
                        editable &&
                        draft.kind !== 'replacement' ? (
                          <SelectField
                            aria-label={t('tasks.row.decisionLabel', {
                              filename: filename(row.source),
                            })}
                            value={row.action}
                            disabled={busy}
                            onValueChange={(value) =>
                              changeRows(new Set([row.id]), {
                                action: value,
                              })
                            }
                          >
                            {Object.entries(actions).map(([value, label]) => (
                              <SelectOption key={value} value={value}>
                                {label}
                              </SelectOption>
                            ))}
                          </SelectField>
                        ) : null}
                        {!row.parent_id &&
                        editable &&
                        draft.kind !== 'replacement' ? (
                          <SelectField
                            aria-label={t('tasks.row.operationLabel', {
                              filename: filename(row.source),
                            })}
                            value={row.operation || 'copy'}
                            disabled={busy || row.action === 'skip'}
                            onValueChange={(value) =>
                              changeRows(new Set([row.id]), {
                                operation: value as PlanRow['operation'],
                              })
                            }
                          >
                            {Object.entries(operations)
                              .filter(
                                ([value]) =>
                                  value !== 'adopt' && value !== 'rename',
                              )
                              .map(([value, label]) => (
                                <SelectOption key={value} value={value}>
                                  {label}
                                </SelectOption>
                              ))}
                          </SelectField>
                        ) : (
                          <small>
                            {row.action === 'skip'
                              ? t('common.actions.skip')
                              : operations[row.operation || 'copy']}
                          </small>
                        )}
                        <Status state={row.state} />
                        {row.remote && (
                          <small>
                            {row.remote.method === 'rename'
                              ? t('tasks.transfer.rename')
                              : row.remote.method === 'smb_move'
                                ? t('tasks.transfer.serverMove')
                                : t('tasks.transfer.serverCopy')}
                          </small>
                        )}
                        {row.warnings.length > 0 && (
                          <>
                            <ul className="warnings">
                              {row.warnings.map((w) => (
                                <li key={backendMessage(w)}>
                                  {backendMessage(w)}
                                </li>
                              ))}
                            </ul>
                            <Label className="check-label">
                              <Checkbox
                                disabled={
                                  !editable || draft.kind === 'replacement'
                                }
                                checked={row.reviewed}
                                onCheckedChange={(v) =>
                                  setDraft({
                                    ...draft,
                                    rows: draft.rows.map((r) =>
                                      r.id === row.id
                                        ? { ...r, reviewed: v === true }
                                        : r,
                                    ),
                                  })
                                }
                              />
                              {t('tasks.review.warningsConfirmed')}
                            </Label>
                          </>
                        )}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
            <Pages page={page} count={rows.length} onChange={setPage} />
            <Dialog open={confirm} onOpenChange={setConfirm}>
              <DialogContent
                style={{ maxWidth: 'min(560px, calc(100vw - 32px))' }}
              >
                <DialogTitle>{t('tasks.confirm.title')}</DialogTitle>
                <DialogDescription>
                  {renaming ? (
                    t(
                      'tasks.confirm.renameHint',
                    )
                  ) : (
                    <>
                      {t('tasks.confirm.backupHint', {
                        operationHint: draft.rows.some(
                          (r) => r.action !== 'skip' && r.operation === 'move',
                        )
                          ? t(
                              'tasks.confirm.moveHint',
                            )
                          : t('tasks.confirm.copyHint'),
                      })}
                    </>
                  )}
                </DialogDescription>
                <div className="confirm-summary">
                  <p>
                    {t('common.files.countAndSize', {
                      count: draft.rows.filter((r) => r.action !== 'skip')
                        .length,
                      size: bytes(
                        draft.rows
                          .filter((r) => r.action !== 'skip')
                          .reduce((n, r) => n + r.signature.size, 0),
                      ),
                    })}
                  </p>
                  <div className="transfer-summary">
                    {Object.entries(operations).map(([key, label]) => {
                      const count = draft.rows.filter(
                        (r) =>
                          r.action !== 'skip' &&
                          (r.operation || 'copy') === key,
                      ).length
                      return (
                        count > 0 && (
                          <p key={key}>
                            {t('tasks.confirm.operationCount', {
                              operation: label,
                              count: count,
                            })}
                          </p>
                        )
                      )
                    })}
                  </div>
                  {draft.rows.some(
                    (r) => r.action !== 'skip' && r.operation === 'symlink',
                  ) && (
                    <p>
                      {t(
                        'tasks.confirm.symlinkHint',
                      )}
                    </p>
                  )}
                  <p>
                    {t('tasks.verification.summary', {
                      method: draft.verify_hash
                        ? t('tasks.verification.fullHash')
                        : renaming
                          ? t('tasks.verification.identity')
                          : t(
                              'tasks.verification.default',
                            ),
                    })}
                  </p>
                  <p className="path-text">
                    {draft.backup_root ? (
                      <FilePath
                        path={draft.backup_root}
                        label={t('tasks.backup.directory')}
                      />
                    ) : renaming ? (
                      t('tasks.backup.noNfo')
                    ) : (
                      t('tasks.backup.notConfigured')
                    )}
                  </p>
                </div>
                <div className="dialog-buttons">
                  <DialogClose asChild>
                    <Button variant="secondary" data-tone="gray">
                      {t('tasks.confirm.back')}
                    </Button>
                  </DialogClose>
                  <Button
                    disabled={busy || running || terminal || stopping}
                    onClick={() => void start()}
                  >
                    {t('tasks.confirm.start')}
                  </Button>
                </div>
              </DialogContent>
            </Dialog>
          </>
        ) : (
          <Empty title={t('tasks.empty.title')}>
            {t('tasks.empty.hint')}
          </Empty>
        )}
      </Card>
      <Dialog
        open={deleting.length > 0}
        onOpenChange={(open) => {
          if (!open && !busy) setDeleteIds([])
        }}
      >
        <DialogContent style={{ maxWidth: 'min(480px, calc(100vw - 32px))' }}>
          <DialogTitle>
            {deleting.length > 1
              ? t('tasks.cleanup.confirmTitle', { count: deleting.length })
              : t('tasks.delete.confirmTitle')}
          </DialogTitle>
          <DialogDescription>
            {t(
              'tasks.delete.description',
            )}
          </DialogDescription>
          <p>
            {deleting.length > 1
              ? t('tasks.cleanup.summary', {
                  completed: deleting.filter((p) => p.state === 'complete').length,
                  cancelled: deleting.filter((p) => p.state === 'cancelled')
                    .length,
                })
              : deleting[0]?.works.map((w) => w.title).join('、')}
          </p>
          <div className="dialog-buttons">
            <DialogClose asChild>
              <Button variant="secondary" disabled={busy}>
                {t('common.actions.back')}
              </Button>
            </DialogClose>
            <Button
              variant="destructive"
              disabled={
                busy ||
                running ||
                !deleting.length ||
                deleting.some(
                  (p) => !['complete', 'cancelled'].includes(p.state),
                )
              }
              onClick={() =>
                void run(t('tasks.delete.records'), async () => {
                  const ids = deleting.map((p) => p.id)
                  if (!ids.length) return
                  if (ids.length === 1)
                    await call('delete_plan', { id: ids[0] })
                  else await call('delete_plans', { ids })
                  setDeleteIds([])
                  onDelete(ids)
                })
              }
            >
              {t('common.actions.confirmDeletePermanently')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
