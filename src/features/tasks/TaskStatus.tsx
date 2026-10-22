import { Badge } from '@/components/ui/badge'
import { t } from '@/i18n'

const states: Record<string, string> = {
  get draft() {
    return t('tasks.status.needsConfirmation')
  },
  get queued() {
    return t('tasks.status.queued')
  },
  get running() {
    return t('common.status.processing')
  },
  get pending() {
    return t('tasks.status.pending')
  },
  get staging() {
    return t('tasks.status.staging')
  },
  get staged() {
    return t('tasks.status.verified')
  },
  get backing_up() {
    return t('tasks.status.backingUp')
  },
  get removing_source() {
    return t('tasks.status.removingSource')
  },
  get committing() {
    return t('tasks.status.committing')
  },
  get complete() {
    return t('tasks.summary.completed')
  },
  get failed() {
    return t('tasks.status.retryRequired')
  },
  get paused() {
    return t('tasks.status.paused')
  },
  get cancelling() {
    return t('tasks.status.cancelling')
  },
  get cancelled() {
    return t('tasks.status.cancelled')
  },
  get skipped() {
    return t('tasks.status.skipped')
  },
  get metadata_failed() {
    return t('tasks.metadata.retryRequired')
  },
}

export function Status({ state }: { state: string }) {
  return (
    <Badge
      tone={
        state === 'complete'
          ? 'green'
          : ['failed', 'metadata_failed', 'cancelled'].includes(state)
            ? 'red'
            : [
                  'running',
                  'staging',
                  'staged',
                  'backing_up',
                  'removing_source',
                  'committing',
                  'cancelling',
                ].includes(state)
              ? 'blue'
              : 'amber'
      }
    >
      {states[state] || state}
    </Badge>
  )
}
