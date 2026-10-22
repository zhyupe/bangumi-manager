import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { useEffect, useState } from 'react'
import { FilePath } from '@/components/file-path'
import MediaDetails from '@/components/media-details'
import { SelectField, SelectOption } from '@/components/select-field'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import { call } from '@/lib/api'
import { bytes, filename } from '@/lib/format'
import type {
  Plan,
  ReplacementCandidate,
  ReplacementRequest,
  ReplacementTarget,
  Run,
} from '@/types'
import { operationLabels } from './resource-model'

export default function ReplacementDialog({
  target,
  onClose,
  onPlan,
  run,
  busy,
}: {
  target: ReplacementTarget | null
  onClose: () => void
  onPlan: (p: Plan) => void
  run: Run
  busy: boolean
}) {
  const [candidates, setCandidates] = useState<ReplacementCandidate[]>([])
  const [choices, setChoices] = useState<Record<string, string>>({})
  const [errors, setErrors] = useState<string[]>([])
  const [preview, setPreview] = useState<Plan | null>(null)
  const [checked, setChecked] = useState<Set<string>>(new Set())
  const [replaceSubtitles, setReplaceSubtitles] = useState(false)
  const [loading, setLoading] = useState(false)
  const [probing, setProbing] = useState(false)
  const [probeProgress, setProbeProgress] = useState({ done: 0, total: 0 })
  useEffect(() => {
    setCandidates([])
    setChoices({})
    setErrors([])
    setPreview(null)
    setChecked(new Set())
    setReplaceSubtitles(false)
    if (!target) return
    let disposed = false
    setLoading(true)
    const load = async () => {
      if (target.manual && target.paths.length === 1) {
        const path = await open({
          multiple: false,
          title: t('resources.replace.chooseMedia'),
        })
        if (!disposed && typeof path === 'string')
          setChoices({ [target.paths[0]]: path })
      } else {
        const result = await call<{
          candidates: ReplacementCandidate[]
          errors: string[]
        }>('scan_replacements', { paths: target.paths })
        if (!disposed) {
          setCandidates(result.candidates)
          setErrors(result.errors)
        }
      }
    }
    void load()
      .catch((e) => {
        if (!disposed) setErrors([String(e)])
      })
      .finally(() => {
        if (!disposed) setLoading(false)
      })
    return () => {
      disposed = true
    }
  }, [target])
  const requests: ReplacementRequest[] = (target?.paths || [])
    .filter((p) => choices[p])
    .map((p) => ({
      old_path: p,
      new_path: choices[p],
      replace_subtitles: replaceSubtitles,
    }))
  const build = async () => {
    const plan = await call<Plan>('preview_replacements', { requests })
    setPreview(plan)
    setChecked(new Set())
  }
  const ready = preview?.rows.every((row) =>
    row.warnings.every((_, i) => checked.has(`${row.id}:${i}`)),
  )
  return (
    <Dialog
      open={!!target}
      onOpenChange={(v) => {
        if (!v && !busy && !loading) onClose()
      }}
    >
      <DialogContent className="replacement-dialog">
        <DialogHeader>
          <DialogTitle>
            {preview ? t('resources.replace.reviewTitle') : t('resources.replace.selectTitle')}
          </DialogTitle>
          <DialogDescription>
            {t(
              'resources.replace.description',
            )}
          </DialogDescription>
        </DialogHeader>
        <div className="replacement-body">
          {loading && <p role="status">{t('resources.replace.searching')}</p>}
          {errors.map((e, i) => (
            <Alert variant="destructive" key={i}>
              <AlertDescription>{backendMessage(e)}</AlertDescription>
            </Alert>
          ))}
          {!preview &&
            target?.paths.map((path) => (
              <Card className="replacement-choice" key={path}>
                <strong>{filename(path)}</strong>
                <FilePath path={path} />
                <div className="replacement-pick">
                  <SelectField
                    aria-label={t('resources.replace.item', { filename: filename(path) })}
                    value={choices[path] || '__skip'}
                    onValueChange={(value) =>
                      setChoices((c) => ({
                        ...c,
                        [path]: value === '__skip' ? '' : value,
                      }))
                    }
                    disabled={loading || busy}
                  >
                    <SelectOption value="__skip">
                      {t('resources.replace.keepOriginal')}
                    </SelectOption>
                    {[
                      ...new Set([
                        ...candidates
                          .filter((c) => c.old_path === path)
                          .map((c) => c.new_path),
                        ...(choices[path] ? [choices[path]] : []),
                      ]),
                    ].map((newPath) => (
                      <SelectOption key={newPath} value={newPath}>
                        {filename(newPath)} ·{' '}
                        {backendMessage(
                          candidates.find(
                            (c) =>
                              c.old_path === path && c.new_path === newPath,
                          )?.reason || t('resources.replace.chooseManually'),
                        )}
                      </SelectOption>
                    ))}
                  </SelectField>
                  <Button
                    variant="outline"
                    disabled={loading || busy}
                    onClick={() =>
                      void run(t('resources.replace.chooseFile'), async () => {
                        const result = await open({
                          multiple: false,
                          title: t('resources.replace.chooseMedia'),
                        })
                        if (typeof result === 'string')
                          setChoices((c) => ({ ...c, [path]: result }))
                      })
                    }
                  >
                    {t('common.actions.browse')}
                  </Button>
                </div>
                {choices[path] && (
                  <FilePath
                    path={choices[path]}
                    label={t('resources.replace.newFile')}
                    previousPaths={[path]}
                  />
                )}
              </Card>
            ))}
          {!preview && (
            <Label className="check-label">
              <Checkbox
                checked={replaceSubtitles}
                onCheckedChange={(v) => setReplaceSubtitles(v === true)}
              />
              {t('resources.replace.replaceSubtitles')}
            </Label>
          )}
          {preview?.rows
            .filter((r) => !r.attachment)
            .map((row) => {
              const attachments = preview.rows.filter(
                (r) => r.parent_id === row.id,
              )
              const backups = [
                ...row.backups,
                ...attachments.flatMap((r) => r.backups),
              ]
              const primaryPaths = [
                row.replacement!.old_asset.path,
                row.source,
                row.target,
              ]
              const backupPaths = backups.flatMap((b) => [b.original, b.backup])
              return (
                <Card className="replacement-comparison" key={row.id}>
                  <h3>{filename(row.replacement!.old_asset.path)}</h3>
                  <div className="comparison-columns">
                    <div>
                      <h4>
                        {t('resources.replace.originalSummary', {
                          size: bytes(
                            row.replacement!.old_asset.signature.size,
                          ),
                        })}
                      </h4>
                      <FilePath path={row.replacement!.old_asset.path} />
                      <MediaDetails probe={row.target_probe} />
                    </div>
                    <div>
                      <h4>
                        {t('resources.replace.newSummary', {
                          size: bytes(row.signature.size),
                        })}
                      </h4>
                      <FilePath
                        path={row.source}
                        previousPaths={[row.replacement!.old_asset.path]}
                      />
                      <MediaDetails probe={row.probe} />
                    </div>
                  </div>
                  <p>
                    {t('resources.replace.sizeChange', {
                      direction:
                        row.signature.size >
                        row.replacement!.old_asset.signature.size
                          ? t('resources.replace.sizeIncrease')
                          : t('resources.replace.sizeDecrease'),
                      size: bytes(
                        Math.abs(
                          row.signature.size -
                            row.replacement!.old_asset.signature.size,
                        ),
                      ),
                    })}
                  </p>
                  <p>
                    {t('resources.replace.operation', {
                      operation: operationLabels[row.operation || 'copy'],
                    })}
                  </p>
                  <FilePath
                    path={row.target}
                    label={t('resources.replace.finalPath')}
                    previousPaths={primaryPaths.slice(0, 2)}
                  />
                  <FilePath
                    path={`${preview.backup_root.replace(/[\\/]+$/, '')}/${preview.id}/${row.id}`}
                    label={t('tasks.backup.directory')}
                    openButton={!backups.some((b) => b.backup)}
                  />
                  {backups.map((b, i) => (
                    <p className="path-text muted" key={b.original}>
                      <FilePath
                        path={b.original}
                        label={t('resources.replace.backup')}
                        previousPaths={[
                          ...primaryPaths,
                          ...backupPaths.slice(0, i * 2),
                        ]}
                      />
                      <FilePath
                        path={b.backup}
                        label="→ "
                        previousPaths={[
                          ...primaryPaths,
                          ...backupPaths.slice(0, i * 2 + 1),
                        ]}
                      />
                      {b.preserve_original
                        ? t('resources.replace.preserveMetadata')
                        : ''}
                    </p>
                  ))}
                  {attachments.map((r, i) => (
                    <p className="path-text" key={r.id}>
                      <FilePath
                        path={r.source}
                        label={t('resources.replace.subtitles')}
                        previousPaths={[
                          ...primaryPaths,
                          ...backupPaths,
                          ...attachments
                            .slice(0, i)
                            .flatMap((previous) => [
                              previous.source,
                              previous.target,
                            ]),
                        ]}
                      />
                      <FilePath
                        path={r.target}
                        label="→ "
                        previousPaths={[
                          ...primaryPaths,
                          ...backupPaths,
                          r.source,
                          ...attachments
                            .slice(0, i)
                            .flatMap((previous) => [
                              previous.source,
                              previous.target,
                            ]),
                        ]}
                      />
                    </p>
                  ))}
                </Card>
              )
            })}
          {preview?.rows.flatMap((row) =>
            row.warnings.map((warning, i) => (
              <Alert className="replacement-warning" key={`${row.id}:${i}`}>
                <AlertDescription>
                  <Label className="check-label">
                    <Checkbox
                      checked={checked.has(`${row.id}:${i}`)}
                      onCheckedChange={(value) =>
                        setChecked((current) => {
                          const next = new Set(current)
                          if (value) next.add(`${row.id}:${i}`)
                          else next.delete(`${row.id}:${i}`)
                          return next
                        })
                      }
                    />
                    {t('resources.replace.confirmWarning', {
                      filename: filename(row.source),
                      warning: backendMessage(warning),
                    })}
                  </Label>
                </AlertDescription>
              </Alert>
            )),
          )}
        </div>
        <DialogFooter className="replacement-actions">
          {probing && (
            <>
              <span role="status">
                {t('resources.probe.progress', {
                  completed: probeProgress.done,
                  total: probeProgress.total,
                })}
              </span>
              <Button
                variant="outline"
                onClick={() =>
                  void call('cancel_resource_probe').catch((e) =>
                    setErrors([String(e)]),
                  )
                }
              >
                {t('resources.probe.cancel')}
              </Button>
            </>
          )}
          <Button
            variant="outline"
            disabled={busy || loading}
            onClick={onClose}
          >
            {t('common.actions.cancel')}
          </Button>
          {preview ? (
            <>
              <Button
                variant="outline"
                disabled={busy}
                onClick={() => setPreview(null)}
              >
                {t('resources.replace.backToSelection')}
              </Button>
              <Button
                disabled={busy}
                variant="secondary"
                onClick={() =>
                  void run(t('resources.replace.readMedia'), async () => {
                    const paths = [
                      ...new Set(
                        requests.flatMap((r) => [r.old_path, r.new_path]),
                      ),
                    ]
                    setProbing(true)
                    setProbeProgress({ done: 0, total: paths.length })
                    let unlisten: (() => void) | undefined
                    try {
                      unlisten = await listen<{ done: number; total: number }>(
                        'resource-probe-progress',
                        (e) =>
                          setProbeProgress((p) => ({
                            done: Math.max(p.done, e.payload.done),
                            total: e.payload.total,
                          })),
                      )
                      await call('probe_resources', { paths, force: true })
                    } finally {
                      unlisten?.()
                      setProbing(false)
                    }
                    await build()
                  })
                }
              >
                {t('resources.replace.refreshMedia')}
              </Button>
              <Button
                disabled={busy || !ready}
                onClick={() =>
                  void run(t('resources.replace.createTask'), async () => {
                    const p = await call<Plan>('create_replacement_plan', {
                      id: preview.id,
                      reviewed: preview.rows.map((r) => r.id),
                    })
                    onClose()
                    onPlan(p)
                  })
                }
              >
                {t('resources.rename.confirm')}
              </Button>
            </>
          ) : (
            <Button
              disabled={loading || busy || !requests.length}
              onClick={() => void run(t('resources.replace.preview'), build)}
            >
              {t('resources.replace.compareCount', { count: requests.length })}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
