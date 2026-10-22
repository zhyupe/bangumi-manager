import './nfo-sync.css'
import { useEffect, useMemo, useRef, useState } from 'react'
import { FilePath } from '@/components/file-path'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { SelectField, SelectOption } from '@/components/select-field'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@/components/ui/dialog'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import { call } from '@/lib/api'
import type { Mapping, Run, SourceFile } from '@/types'

interface NfoMatch {
  path: string
  nfo_path: string
  title: string
  kind: string
  ids: Record<string, string>
  episode_count: number
  existing_title: string
  fingerprint: string
  error: string
}
interface SyncResult {
  mappings: Mapping[]
  errors: NfoMatch[]
}

export default function NfoSync({
  files,
  mappings,
  busy,
  run,
  onSynced,
}: {
  files: SourceFile[]
  mappings: Record<string, Mapping>
  busy: boolean
  run: Run
  onSynced: (mappings: Mapping[]) => Promise<void>
}) {
  const inputs = useMemo(
    () =>
      JSON.stringify(
        files.filter((f) => f.media).map((f) => [f.path, f.signature]),
      ),
    [files],
  )
  const [matches, setMatches] = useState<NfoMatch[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [open, setOpen] = useState(false)
  const [revision, setRevision] = useState(0)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [summary, setSummary] = useState('')
  const inspection = useRef<{
    key: string
    request: Promise<NfoMatch[]>
  } | null>(null)
  useEffect(() => {
    let active = true
    const paths = (JSON.parse(inputs) as [string, unknown][]).map(
      ([path]) => path,
    )
    setMatches([])
    setSelected(new Set())
    setError('')
    setSummary('')
    if (!paths.length) {
      setLoading(false)
      return
    }
    setLoading(true)
    const key = `${revision}:${inputs}`
    if (inspection.current?.key !== key) {
      inspection.current = {
        key,
        request: call<NfoMatch[]>('inspect_source_nfos', { paths }),
      }
    }
    void inspection.current.request
      .then((result) => {
        if (!active) return
        setMatches(result)
        const first = result.find((m) => !m.error && !mappings[m.path]) || result[0]
        setSelected(new Set(first ? [first.path] : []))
      })
      .catch((e) => {
        if (active) setError(String(e))
      })
      .finally(() => {
        if (active) setLoading(false)
      })
    return () => {
      active = false
    }
  }, [inputs, revision])
  const available = matches.filter((m) => !m.error && !mappings[m.path])
  const chosen = available.filter((m) => selected.has(m.path))
  if (!files.some((f) => f.media) || (!loading && !error && !matches.length))
    return null
  return (
    <div className="nfo-sync">
      {!!matches.length && (
        <SelectField
          aria-label={t('database.nfo.file')}
          value={[...selected][0] || ''}
          disabled={busy || loading || open}
          onValueChange={(path) => setSelected(new Set([path]))}
        >
          {matches.map((m) => (
            <SelectOption key={m.path} value={m.path}>
              {m.nfo_path || m.path}
            </SelectOption>
          ))}
        </SelectField>
      )}
      <div className="button-row">
        <Button
          size="sm"
          variant="secondary"
          disabled={loading || !matches.length}
          onClick={() => setOpen(true)}
        >
          {loading
            ? t('database.nfo.detecting')
            : t('database.nfo.summary', {
                available: matches.length
                  ? t('database.nfo.availableCount', {
                      count: available.length,
                    })
                  : '',
              })}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          disabled={loading || busy}
          onClick={() => setRevision((n) => n + 1)}
        >
          {t('common.actions.detectAgain')}
        </Button>
      </div>
      {error && (
        <p role="alert">
          {t('database.nfo.detectionFailed', { error: error })}
        </p>
      )}
      <Dialog
        open={open}
        onOpenChange={(value) => {
          if (!busy) setOpen(value)
        }}
      >
        <DialogContent
          className="nfo-sync-dialog"
          showCloseButton={!busy}
          onEscapeKeyDown={(e) => {
            if (busy) e.preventDefault()
          }}
          onInteractOutside={(e) => {
            if (busy) e.preventDefault()
          }}
        >
          <DialogTitle>{t('database.nfo.syncDescription')}</DialogTitle>
          <DialogDescription>{t('database.nfo.description')}</DialogDescription>
          <div className="nfo-sync-list" aria-busy={loading}>
            {loading && <p role="status">{t('database.nfo.reading')}</p>}
            {matches
              .filter((m) => selected.has(m.path))
              .map((m) => (
                <div className="nfo-sync-item" key={m.path}>
                  <div>
                    <strong>{m.title || m.path.split(/[\\/]/).at(-1)}</strong>
                    <p>
                      <FilePath path={m.path} />
                    </p>
                    {m.nfo_path && (
                      <p>
                        <FilePath path={m.nfo_path} label="NFO：" />
                      </p>
                    )}
                    <div className="badge-row">
                      {Object.entries(m.ids)
                        .sort(([a], [b]) =>
                          a === 'tmdb'
                            ? -1
                            : b === 'tmdb'
                              ? 1
                              : a.localeCompare(b),
                        )
                        .map(([provider, id]) => (
                          <Badge tone="gray" key={provider}>
                            {provider.toUpperCase()} #{id}
                          </Badge>
                        ))}
                    </div>
                    {m.error ? (
                      <p role="alert">{backendMessage(m.error)}</p>
                    ) : (
                      <p className="muted">
                        {t('database.nfo.itemSummary', {
                          action: mappings[m.path]
                            ? t('database.nfo.alreadyMatched')
                            : m.existing_title
                              ? t('database.nfo.linkExisting', {
                                  title: m.existing_title,
                                })
                              : t('database.actions.addToLibrary'),
                          provider:
                            m.kind === 'movie'
                              ? t('common.media.movie')
                              : m.kind === 'music'
                                ? t('common.media.music')
                                : t('common.media.series'),
                          count: m.episode_count,
                        })}
                      </p>
                    )}
                  </div>
                </div>
              ))}
          </div>
          {summary && <p role="status">{summary}</p>}
          {error && <p role="alert">{error}</p>}
          <div className="button-row">
            <Button
              disabled={busy || loading || !chosen.length}
              onClick={() =>
                void run(t('database.nfo.title'), async () => {
                  setError('')
                  const result = await call<SyncResult>('sync_source_nfos', {
                    selections: chosen.map(({ path, fingerprint }) => ({
                      path,
                      fingerprint,
                    })),
                  })
                  setSummary(
                    t('database.nfo.syncedSummary', {
                      count: result.mappings.length,
                      errors: result.errors.length
                        ? t('database.nfo.unsyncedCount', {
                            count: result.errors.length,
                          })
                        : '',
                    }),
                  )
                  setMatches((prev) =>
                    prev.map((m) => {
                      const failed = result.errors.find(
                        (e) => e.path === m.path,
                      )
                      return failed ? { ...m, error: failed.error } : m
                    }),
                  )
                  if (result.mappings.length) await onSynced(result.mappings)
                  if (result.mappings.length && !result.errors.length)
                    setOpen(false)
                })
              }
            >
              {t('database.nfo.syncSelected', { count: chosen.length })}
            </Button>
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => setOpen(false)}
            >
              {t('common.actions.close')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
