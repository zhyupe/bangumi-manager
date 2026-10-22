import { useEffect, useState } from 'react'
import { Disclosure, DisclosureTrigger } from '@/components/disclosure'
import { FilePath } from '@/components/file-path'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
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
import type { Plan, Run } from '@/types'

export default function RenameDialog({
  paths,
  onClose,
  onPlan,
  run,
  busy,
}: {
  paths: string[] | null
  onClose: () => void
  onPlan: (plan: Plan) => void
  run: Run
  busy: boolean
}) {
  const [preview, setPreview] = useState<Plan | null>(null)
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    setPreview(null)
    setError('')
    if (!paths) return
    let disposed = false
    setLoading(true)
    void call<Plan>('preview_renames', { paths })
      .then((p) => {
        if (!disposed) setPreview(p)
      })
      .catch((e) => {
        if (!disposed) setError(String(e))
      })
      .finally(() => {
        if (!disposed) setLoading(false)
      })
    return () => {
      disposed = true
    }
  }, [paths, revision])
  const backups = preview?.rows.flatMap((r) => r.backups) || []
  const conflicts = preview?.rows.some((r) => r.conflicts.length)
  return (
    <Dialog
      open={paths !== null}
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent className="replacement-dialog rename-dialog">
        <DialogHeader>
          <DialogTitle>{t('tasks.review.renameTitle')}</DialogTitle>
          <DialogDescription>
            {t(
              'resources.rename.description',
            )}
          </DialogDescription>
        </DialogHeader>
        <div className="replacement-body">
          {loading && <p role="status">{t('resources.rename.loading')}</p>}
          {error && (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
          {preview && (
            <>
              <p>
                {t('resources.rename.summary', {
                  media: preview.rows.filter((r) => !r.attachment).length,
                  attachments: preview.rows.filter((r) => r.attachment).length,
                })}
              </p>
              {conflicts && (
                <Alert variant="destructive">
                  <AlertDescription>
                    {t(
                      'resources.rename.conflictHint',
                    )}
                  </AlertDescription>
                </Alert>
              )}
              {preview.rows.length ? (
                <Table className="rename-table">
                  <TableHeader>
                    <TableRow>
                      <TableHead>{t('resources.paths.original')}</TableHead>
                      <TableHead>{t('resources.paths.new')}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {preview.rows.map((row) => (
                      <TableRow key={row.id}>
                        <TableCell>
                          <FilePath path={row.source} />
                        </TableCell>
                        <TableCell>
                          <FilePath
                            path={row.target}
                            previousPaths={[row.source]}
                          />
                          {row.conflicts.map((c) => (
                            <p className="conflict" key={backendMessage(c)}>
                              {backendMessage(c)}
                            </p>
                          ))}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              ) : (
                <p>{t('resources.rename.unchanged')}</p>
              )}
              {!!backups.length && (
                <Disclosure>
                  <DisclosureTrigger>
                    {t('resources.rename.nfoBackupCount', {
                      count: backups.length,
                    })}
                  </DisclosureTrigger>
                  {backups.map((b) => (
                    <div className="rename-backup" key={b.original}>
                      <FilePath path={b.original} openButton={false} />
                      <FilePath path={b.backup} label="→ " />
                    </div>
                  ))}
                </Disclosure>
              )}
              {!!preview.metadata_notices?.length && (
                <Disclosure>
                  <DisclosureTrigger>
                    {t('resources.rename.unchangedCount', {
                      count: preview.metadata_notices.length,
                    })}
                  </DisclosureTrigger>
                  {preview.metadata_notices.map((message) => (
                    <p
                      className="path-text muted"
                      key={backendMessage(message)}
                    >
                      {backendMessage(message)}
                    </p>
                  ))}
                </Disclosure>
              )}
              {!!preview.rows.length && (
                <p className="muted">
                  {t(
                    'resources.rename.recoveryHint',
                  )}
                </p>
              )}
            </>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={onClose}>
            {t('common.actions.cancel')}
          </Button>
          <Button
            variant="secondary"
            disabled={busy || loading}
            onClick={() => setRevision((r) => r + 1)}
          >
            {t('common.actions.previewAgain')}
          </Button>
          <Button
            disabled={
              busy || loading || !!error || !preview?.rows.length || conflicts
            }
            onClick={() =>
              void run(t('resources.rename.createTask'), async () => {
                if (!preview) return
                try {
                  const plan = await call<Plan>('create_rename_plan', {
                    id: preview.id,
                  })
                  onPlan(plan)
                  onClose()
                } catch (e) {
                  setError(e instanceof Error ? e.message : String(e))
                  throw e
                }
              })
            }
          >
            {t('resources.rename.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
