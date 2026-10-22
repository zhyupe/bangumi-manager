import { Cross2Icon } from '@radix-ui/react-icons'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/components/ui/dialog'
import { t } from '@/i18n'
import type { Run, Work } from '@/types'
import RemotePane from './RemotePane'
import './media-import.css'

export interface ImportTarget {
  kind: string
  keyword?: string
  workId?: string
  episodeGroups?: boolean
}
export default function MediaImportDialog({
  target,
  works,
  busy,
  mutationBusy,
  run,
  onClose,
  onBound,
}: {
  target: ImportTarget | null
  works: Work[]
  busy: boolean
  mutationBusy?: boolean
  run: Run
  onClose: () => void
  onBound: (w: Work) => Promise<void>
}) {
  return (
    <Dialog
      open={!!target}
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent
        className="import-dialog"
        onEscapeKeyDown={(e) => {
          if (busy) e.preventDefault()
        }}
        onInteractOutside={(e) => {
          if (busy) e.preventDefault()
        }}
        showCloseButton={false}
      >
        <header className="import-dialog-header">
          <div className="section-title">
            <DialogTitle>
              {target?.episodeGroups
                ? t('library.groups.switch')
                : target?.workId
                  ? t('library.seasons.add')
                  : t('library.actions.addWork')}
            </DialogTitle>
            <Button
              aria-label={t('database.dialog.close')}
              variant="ghost"
              disabled={busy}
              onClick={onClose}
              size="icon-sm"
            >
              <Cross2Icon />
            </Button>
          </div>
          <DialogDescription>
            {target?.episodeGroups
              ? t('database.dialog.title')
              : t('database.import.keepOpenHint')}
          </DialogDescription>
        </header>
        {target && (
          <RemotePane
            key={`${target.kind}:${target.workId}:${target.keyword}`}
            initialKind={target.kind}
            episodeGroups={target.episodeGroups}
            mutationBusy={mutationBusy}
            existingId={target.workId}
            keyword={target.keyword || ''}
            works={works}
            busy={busy}
            run={run}
            onBound={async (w) => {
              await onBound(w)
              if (target.episodeGroups) onClose()
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}
