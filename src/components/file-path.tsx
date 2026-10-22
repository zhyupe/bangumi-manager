import { OpenInNewWindowIcon } from '@radix-ui/react-icons'
import { createContext, useContext, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/spinner'
import { t } from '@/i18n'
import { call, desktop } from '@/lib/api'
import { cn } from '@/lib/utils'
import type { Notify } from './notifications'

export const FileActionsContext = createContext<Notify | null>(null)

export function OpenPathButton({
  path,
  disabled = false,
}: {
  path: string
  disabled?: boolean
}) {
  const notify = useContext(FileActionsContext)
  const [opening, setOpening] = useState(false)
  const pending = useRef(false)
  const label = t('common.paths.open', { path: path })
  return (
    <Button
      variant="ghost"
      size="icon-xs"
      className="open-path-button"
      title={label}
      aria-label={label}
      aria-busy={opening}
      disabled={disabled || !desktop || !path.trim() || opening}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={async (event) => {
        event.preventDefault()
        event.stopPropagation()
        if (pending.current) return
        pending.current = true
        setOpening(true)
        try {
          const result = await call<{ path: string; fallback: boolean }>(
            'open_in_file_manager',
            { path },
          )
          if (result.fallback)
            notify?.(
              'warning',
              t('common.paths.openedParent', {
                path: result.path,
              }),
            )
        } catch (error) {
          notify?.(
            'error',
            t('common.paths.openFailed', {
              path: path,
              error: String(error),
            }),
          )
        } finally {
          pending.current = false
          setOpening(false)
        }
      }}
    >
      {opening ? (
        <Spinner aria-hidden="true" />
      ) : (
        <OpenInNewWindowIcon className="size-4" aria-hidden="true" />
      )}
    </Button>
  )
}

export function FilePath({
  path,
  label,
  truncate = false,
  className,
  openButton = true,
  previousPaths = [],
}: {
  path: string
  label?: string
  truncate?: boolean
  className?: string
  openButton?: boolean
  previousPaths?: string[]
}) {
  const key = (value: string) => {
    const normalized = value.replace(/\\/g, '/').replace(/\/+$/, '')
    return /^(?:[a-z]:\/|\/\/)/i.test(normalized)
      ? normalized.toLowerCase()
      : normalized
  }
  const duplicate = previousPaths.some(
    (previous) => key(previous) === key(path),
  )
  return (
    <span
      className={cn(
        'path-text file-path',
        truncate && 'file-path-truncate',
        className,
      )}
    >
      <span className="file-path-value" title={path}>
        {label}
        {path}
      </span>
      {openButton && !duplicate && <OpenPathButton path={path} />}
    </span>
  )
}
