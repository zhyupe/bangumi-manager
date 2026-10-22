import {
  CheckCircledIcon,
  Cross2Icon,
  CrossCircledIcon,
  ExclamationTriangleIcon,
} from '@radix-ui/react-icons'
import { useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Alert } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
export type NoticeTone = 'success' | 'error' | 'warning'
export type Notice = {
  id: string
  tone: NoticeTone
  text: string
  action?: { label: string; run: () => void }
}
export type Notify = (
  tone: NoticeTone,
  text: string,
  action?: Notice['action'],
  key?: string,
) => void
export function useNotifications() {
  const [items, setItems] = useState<Notice[]>([])
  const next = useRef(0)
  const dismiss = useCallback(
    (id: string) => setItems((items) => items.filter((n) => n.id !== id)),
    [],
  )
  const notify: Notify = useCallback((tone, text, action, key) => {
    const id = key || String(++next.current)
    setItems((items) =>
      [
        ...items.filter(
          (n) => n.id !== id && (n.tone !== tone || n.text !== text),
        ),
        { id, tone, text, action },
      ].slice(-3),
    )
  }, [])
  return { items, notify, dismiss }
}
function Notification({
  item,
  dismiss,
}: {
  item: Notice
  dismiss: (id: string) => void
}) {
  const [held, setHeld] = useState(false)
  useEffect(() => {
    if (held || item.tone === 'error') return
    const timeout = setTimeout(
      () => dismiss(item.id),
      item.tone === 'success' ? 7000 : 10000,
    )
    return () => clearTimeout(timeout)
  }, [item, held, dismiss])
  const Icon =
    item.tone === 'success'
      ? CheckCircledIcon
      : item.tone === 'error'
        ? CrossCircledIcon
        : ExclamationTriangleIcon
  return (
    <Alert
      className="floating-notice"
      data-app-feedback=""
      data-tone={item.tone}
      role={item.tone === 'error' ? 'alert' : 'status'}
      onMouseEnter={() => setHeld(true)}
      onMouseLeave={() => setHeld(false)}
      onFocusCapture={() => setHeld(true)}
      onBlurCapture={() => setHeld(false)}
    >
      <Icon aria-hidden="true" />
      <div className="floating-notice-body">
        <strong>
          {item.tone === 'success'
            ? t('common.notifications.success')
            : item.tone === 'error'
              ? t('common.status.failed')
              : t('common.notifications.warning')}
        </strong>
        <div className="floating-notice-text">{backendMessage(item.text)}</div>
        {item.action && (
          <Button size="sm" variant="outline" onClick={item.action.run}>
            {item.action.label}
          </Button>
        )}
      </div>
      <Button
        size="icon-xs"
        variant="ghost"
        aria-label={item.tone === 'error' ? t('common.notifications.closeError') : t('common.notifications.dismiss')}
        onClick={() => dismiss(item.id)}
      >
        <Cross2Icon />
      </Button>
    </Alert>
  )
}
export function Notifications({
  items,
  dismiss,
  activity,
  progress,
}: {
  items: Notice[]
  dismiss: (id: string) => void
  activity?: string
  progress?: string
}) {
  return createPortal(
    <section
      className="notification-viewport"
      data-app-feedback=""
      aria-label={t('common.notifications.label')}
      aria-live="polite"
    >
      {activity && (
        <Alert
          className="floating-notice activity"
          data-app-feedback=""
          role="status"
        >
          <Spinner aria-hidden="true" />
          <div className="floating-notice-body">
            <strong>{activity}</strong>
            {progress && <div className="floating-notice-text">{progress}</div>}
          </div>
        </Alert>
      )}
      {items.map((item) => (
        <Notification key={item.id} item={item} dismiss={dismiss} />
      ))}
    </section>,
    document.body,
  )
}
