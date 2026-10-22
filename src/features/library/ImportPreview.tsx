import { BackpackIcon } from '@radix-ui/react-icons'
import { useEffect, useState } from 'react'
import { ActionSplitButton } from '@/components/action-split-button'
import { Disclosure, DisclosureTrigger } from '@/components/disclosure'
import { FilePath } from '@/components/file-path'
import { SelectField, SelectOption } from '@/components/select-field'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import { operationLabels } from '@/features/resources/resource-model'
import { useRememberedAction } from '@/hooks/use-remembered-action'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import { bytes, filename } from '@/lib/format'
import { episodePosition } from '@/lib/media'
import { changeImportRows, importReady, type SubmitMode } from './import-model'
import type { useLibraryImport } from './use-library-import'
import './library-import.css'

export default function ImportPreview({
  draft,
  disabled,
  needsSave = false,
  needsRefresh = false,
  onSave,
}: {
  draft: ReturnType<typeof useLibraryImport>
  disabled: boolean
  needsSave?: boolean
  needsRefresh?: boolean
  onSave?: () => Promise<void>
}) {
  const [open, setOpen] = useState(false)
  const [mode, setMode] = useRememberedAction<SubmitMode>(
    'bangumi.librarySubmitMode',
    ['view', 'execute'],
    'view',
  )
  const { preview, error, scanErrors } = draft
  useEffect(() => {
    if (!preview && !error && !scanErrors.length) setOpen(false)
  }, [preview, error, scanErrors.length])
  if (!preview && !error && !scanErrors.length) return null
  const independent = preview?.rows.filter((r) => !r.parent_id) || []
  return (
    <>
      <Button className="library-import-cart" onClick={() => setOpen(true)}>
        <BackpackIcon />
        {t('library.import.cart', { count: independent.length })}
      </Button>
      <Dialog
        open={open}
        onOpenChange={(value) => {
          if (!disabled) setOpen(value)
        }}
      >
        <DialogContent
          className="library-import-dialog"
          showCloseButton={!disabled}
        >
          <header className="library-import-heading">
            <DialogTitle>{t('library.import.pendingFiles')}</DialogTitle>
            <DialogDescription>
              {t('library.import.reviewHint')}
            </DialogDescription>
          </header>
          {!preview && (
            <Button variant="ghost" disabled={disabled} onClick={draft.clear}>
              {t('library.import.discard')}
            </Button>
          )}
          <section
            className="library-import-content"
            aria-label={t('library.import.pendingFiles')}
          >
            {needsSave && (
              <p className="muted">{t('library.import.unsavedChanges')}</p>
            )}
            {error && (
              <Alert variant="destructive">
                <AlertDescription>{backendMessage(error)}</AlertDescription>
              </Alert>
            )}
            {scanErrors.map((message) => (
              <Alert key={message}>
                <AlertDescription>{backendMessage(message)}</AlertDescription>
              </Alert>
            ))}
            {independent.map((row) => {
              const work = preview!.works.find((w) => w.id === row.work_id)
              const eps =
                work?.episodes.filter((e) => row.episode_ids.includes(e.id)) ||
                []
              const attachments = preview!.rows.filter(
                (r) => r.parent_id === row.id,
              )
              const change = (
                changes: Parameters<typeof changeImportRows>[2],
              ) => draft.setPreview(changeImportRows(preview!, row.id, changes))
              return (
                <div
                  className="library-import-row"
                  key={row.id}
                  data-skipped={row.action === 'skip'}
                >
                  <div className="library-import-paths">
                    <div className="library-import-title">
                      <strong>{filename(row.source)}</strong>
                      <Badge tone="gray">
                        {row.extra
                          ? t('common.files.extra')
                          : row.subtitle
                            ? t('common.files.subtitle')
                            : eps
                                .map((ep) =>
                                  episodePosition(ep, work?.kind || 'tv'),
                                )
                                .join(' · ')}
                      </Badge>
                      {!!row.part && <Badge tone="gray">Part {row.part}</Badge>}
                      <span className="muted">{bytes(row.signature.size)}</span>
                    </div>
                    <FilePath path={row.source} />
                    {(row.target_signature ||
                      row.backups.length > 0 ||
                      row.conflicts.length > 0) && (
                      <FilePath
                        path={row.target}
                        label={t('resources.replace.finalPath')}
                        previousPaths={[row.source]}
                      />
                    )}
                    {row.target_signature && (
                      <p>
                        {t('tasks.target.existingSize', {
                          size: bytes(row.target_signature.size),
                        })}
                      </p>
                    )}
                    {row.subtitle && (
                      <FilePath
                        path={row.subtitle.asset.path}
                        label={t('organize.subtitles.targetVideo')}
                      />
                    )}
                    {row.backups.map((backup) => (
                      <FilePath
                        key={backup.original}
                        path={backup.original}
                        label={t('common.backup.planned')}
                        previousPaths={[row.target]}
                      />
                    ))}
                    {row.conflicts.map((message) => (
                      <p className="conflict" key={message}>
                        {backendMessage(message)}
                      </p>
                    ))}
                    {attachments.length > 0 && (
                      <Disclosure
                        defaultOpen={attachments.some(
                          (sub) => sub.conflicts.length > 0,
                        )}
                      >
                        <DisclosureTrigger>
                          {t('library.import.attachedSubtitles', {
                            count: attachments.length,
                          })}
                        </DisclosureTrigger>
                        {attachments.map((sub) => (
                          <div key={sub.id}>
                            <FilePath path={sub.source} />
                            <FilePath
                              path={sub.target}
                              label="→ "
                              previousPaths={[sub.source]}
                            />
                            {sub.conflicts.map((message) => (
                              <p className="conflict" key={message}>
                                {backendMessage(message)}
                              </p>
                            ))}
                          </div>
                        ))}
                      </Disclosure>
                    )}
                    {row.warnings.length > 0 && (
                      <div className="library-import-warnings">
                        <ul>
                          {row.warnings.map((message) => (
                            <li key={message}>{backendMessage(message)}</li>
                          ))}
                        </ul>
                        <Label className="check-label">
                          <Checkbox
                            disabled={disabled}
                            checked={row.reviewed}
                            onCheckedChange={(value) =>
                              change({ reviewed: value === true })
                            }
                          />
                          {t('tasks.review.warningsConfirmed')}
                        </Label>
                      </div>
                    )}
                  </div>
                  <div className="library-import-decisions">
                    {(row.action === 'undecided' ||
                      row.action === 'replace' ||
                      row.target_signature ||
                      row.backups.length > 0 ||
                      row.conflicts.length > 0) && (
                      <SelectField
                        aria-label={t('tasks.row.decisionLabel', {
                          filename: filename(row.source),
                        })}
                        value={row.action}
                        disabled={disabled}
                        onValueChange={(action) => change({ action })}
                      >
                        <SelectOption value="undecided">
                          {t('tasks.status.needsConfirmation')}
                        </SelectOption>
                        <SelectOption value="copy">
                          {t('tasks.decisions.keep')}
                        </SelectOption>
                        <SelectOption value="replace">
                          {t('tasks.decisions.replace')}
                        </SelectOption>
                        <SelectOption value="skip">
                          {t('common.actions.skip')}
                        </SelectOption>
                      </SelectField>
                    )}
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={disabled}
                      onClick={() => void draft.remove(row.source)}
                    >
                      {t('common.actions.removeFile')}
                    </Button>
                  </div>
                </div>
              )
            })}
          </section>
          {preview && (
            <footer className="library-import-footer">
              <div className="library-import-operation">
                <Label>{t('library.import.operation')}</Label>
                <SelectField
                  aria-label={t('library.import.operation')}
                  value={
                    new Set(independent.map((r) => r.operation || 'copy'))
                      .size > 1
                      ? 'mixed'
                      : independent[0]?.operation || 'copy'
                  }
                  disabled={disabled}
                  onValueChange={(operation) =>
                    draft.setPreview({
                      ...preview,
                      rows: preview.rows.map((r) => ({
                        ...r,
                        operation: operation as typeof r.operation,
                      })),
                    })
                  }
                >
                  {new Set(independent.map((r) => r.operation || 'copy')).size >
                    1 && (
                    <SelectOption value="mixed">
                      {t('library.import.mixedOperations')}
                    </SelectOption>
                  )}
                  {(['copy', 'move', 'hardlink', 'symlink'] as const).map(
                    (op) => (
                      <SelectOption key={op} value={op}>
                        {operationLabels[op]}
                      </SelectOption>
                    ),
                  )}
                </SelectField>
                {!importReady(preview) && (
                  <p className="muted">{t('library.import.unresolvedFiles')}</p>
                )}
              </div>
              <div className="library-import-submit">
                <Button
                  variant="ghost"
                  disabled={disabled}
                  onClick={draft.clear}
                >
                  {t('library.import.discard')}
                </Button>
                <Button
                  variant="outline"
                  disabled={disabled}
                  onClick={() => void (needsSave ? onSave?.() : draft.refresh())}
                >
                  {t(needsSave ? 'library.import.saveAndPreview' : 'common.actions.previewAgain')}
                </Button>
                <ActionSplitButton
                  value={mode}
                  labels={{
                    view: t('common.submit.view'),
                    execute: t('common.submit.execute'),
                  }}
                  disabled={disabled || needsSave || needsRefresh || !importReady(preview)}
                  switchDisabled={disabled}
                  label={t('common.submit.switchAction')}
                  onAction={() => void draft.submit(mode)}
                  onChange={setMode}
                />
              </div>
            </footer>
          )}
        </DialogContent>
      </Dialog>
    </>
  )
}
