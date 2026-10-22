import { TrashIcon } from '@radix-ui/react-icons'
import { useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/components/ui/dialog'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import type { LibraryIndex, Plan, Run, Work } from '@/types'

export default function DeleteWorkButton({
  work,
  index,
  plans,
  disabled,
  run,
  onDelete,
}: {
  work: Work
  index: LibraryIndex
  plans: Plan[]
  disabled: boolean
  run: Run
  onDelete: (id: string) => Promise<void>
}) {
  const [open, setOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [error, setError] = useState('')
  const pending = useRef(false)
  const hasResources =
    index.assets.some((a) => a.work_id === work.id) ||
    index.resources?.some((r) => r.asset?.work_id === work.id)
  const hasTask = plans.some(
    (p) =>
      !['complete', 'cancelled'].includes(p.state) &&
      (p.rows.some((r) => r.work_id === work.id) ||
        p.works.some((w) => w.id === work.id)),
  )
  const reason = hasResources
    ? t('library.delete.hasResources')
    : hasTask
      ? t('library.delete.hasTasks')
      : ''
  return (
    <div className="mt-2 grid gap-2 border-t pt-4">
      <Button
        variant="outline"
        className="text-destructive"
        disabled={disabled || deleting || !!reason}
        title={reason || undefined}
        onClick={() => {
          setError('')
          setOpen(true)
        }}
      >
        <TrashIcon aria-hidden="true" />
        {t('library.delete.action')}
      </Button>
      {reason && <p className="text-xs text-muted-foreground">{reason}</p>}
      <Dialog
        open={open}
        onOpenChange={(value) => {
          if (!pending.current) setOpen(value)
        }}
      >
        <DialogContent className="sm:max-w-[480px]" showCloseButton={!deleting}>
          <DialogTitle>{t('library.delete.title')}</DialogTitle>
          <DialogDescription>
            {t(
              'library.delete.description',
            )}
          </DialogDescription>
          <p className="break-words font-semibold">{work.title}</p>
          {error && (
            <p className="error-text break-words" role="alert">
              {error}
            </p>
          )}
          {reason && (
            <p className="error-text" role="alert">
              {reason}
            </p>
          )}
          <div className="dialog-buttons">
            <Button
              variant="secondary"
              disabled={deleting}
              onClick={() => setOpen(false)}
            >
              {t('common.actions.cancel')}
            </Button>
            <Button
              variant="destructive"
              disabled={disabled || deleting || !!reason}
              onClick={() => {
                if (pending.current) return
                pending.current = true
                setDeleting(true)
                void run(t('library.delete.action'), async () => {
                  setError('')
                  try {
                    await onDelete(work.id)
                    setOpen(false)
                  } catch (e) {
                    setError(backendMessage(e))
                  } finally {
                    pending.current = false
                    setDeleting(false)
                  }
                })
              }}
            >
              {deleting ? t('library.delete.pending') : t('common.actions.confirmDelete')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
