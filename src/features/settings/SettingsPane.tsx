import { CheckIcon } from '@radix-ui/react-icons'
import { open } from '@tauri-apps/plugin-dialog'
import { useEffect, useState } from 'react'
import { Disclosure, DisclosureTrigger } from '@/components/disclosure'
import { Field } from '@/components/field'
import { FilePath, OpenPathButton } from '@/components/file-path'
import { SelectField, SelectOption } from '@/components/select-field'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Separator } from '@/components/ui/separator'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'
import {
  getLanguagePreference,
  type LanguagePreference,
  setLanguagePreference,
  t,
} from '@/i18n'
import { call } from '@/lib/api'
import { empty } from '@/lib/defaults'
import { defaultSubtitlePattern } from '@/lib/subtitles'
import type { Run, Settings } from '@/types'
import DiagnosticsPane from './DiagnosticsPane'
import { audioToolDirectory, audioToolPaths } from './audio-tool-paths'
export default function SettingsPane({
  settings,
  dataDir,
  run,
  busy,
  onSaved,
}: {
  settings: Settings
  dataDir: string
  run: Run
  busy: boolean
  onSaved: () => Promise<void>
}) {
  const [draft, setDraft] = useState({ ...empty.settings, ...settings })
  const [toolVersion, setToolVersion] = useState('')
  const toolDirectory = audioToolDirectory(draft.ffmpeg || '', draft.ffprobe)
  const [toolDirectoryInput, setToolDirectoryInput] = useState(toolDirectory || '')
  const setExecutable = (tool: 'ffmpeg' | 'ffprobe', value: string) => {
    const next = { ...draft, [tool]: value }
    setDraft(next)
    setToolDirectoryInput(audioToolDirectory(next.ffmpeg || '', next.ffprobe) || '')
    setToolVersion('')
  }
  const setToolDirectory = (directory: string) => {
    setToolDirectoryInput(directory)
    setDraft(d => ({ ...d, ...audioToolPaths(directory) }))
    setToolVersion('')
  }
  const [templateKind, setTemplateKind] = useState<'tv' | 'movie' | 'music'>(
    'tv',
  )
  const [preview, setPreview] = useState<Record<string, string>>({}),
    [templateError, setTemplateError] = useState('')
  useEffect(() => setDraft({ ...empty.settings, ...settings }), [settings])
  useEffect(() => {
    let active = true
    const timer = setTimeout(() => {
      void call<Record<string, string>>('preview_naming', { settings: draft })
        .then((p) => {
          if (active) {
            setPreview(p)
            setTemplateError('')
          }
        })
        .catch((e) => {
          if (active) setTemplateError(String(e))
        })
    }, 350)
    return () => {
      active = false
      clearTimeout(timer)
    }
  }, [draft])
  const roots: [keyof Settings, string, string][] = [
    [
      'output_root',
      t('settings.storage.outputRoot'),
      t('settings.storage.pathPlaceholder'),
    ],
    ['extras_root', t('settings.storage.extrasRoot'), t('settings.storage.extrasHint')],
    ['backup_root', t('settings.storage.backupRoot'), t('settings.storage.externalHint')],
  ]
  const choose = (field: keyof Settings, directory = true) =>
    run(t('common.actions.choosePath'), async () => {
      const path = await open({ directory, multiple: false })
      if (typeof path === 'string') {
        if (field === 'ffmpeg' || field === 'ffprobe') setExecutable(field, path)
        else setDraft((d) => ({ ...d, [field]: path }))
      }
    })
  return (
    <div className="settings-layout">
      <header className="settings-actions" aria-label={t('settings.actions.label')}>
        <span className="settings-save-status" role="status">
          {t(JSON.stringify(draft) === JSON.stringify({ ...empty.settings, ...settings })
            ? 'settings.status.saved' : 'settings.status.unsaved')}
        </span>
        <Button
          disabled={busy || !!templateError}
          onClick={() =>
            void run(t('settings.actions.save'), async () => {
              if (draft.subtitle_language_pattern?.trim()) {
                try {
                  new RegExp(draft.subtitle_language_pattern, 'i')
                } catch {
                  throw new Error(t('errors.subtitles.invalidPattern'))
                }
              }
              await call('save_settings', { settings: draft })
              await onSaved()
            })
          }
        >
          <CheckIcon />
          {t('settings.actions.save')}
        </Button>
      </header>
      <Card className="panel">
        <div className="section-title">
          <div>
            <h2>{t('settings.storage.title')}</h2>
            <p>{t('settings.storage.description')}</p>
          </div>
        </div>
        {roots.map(([key, label, hint]) => (
          <Field key={key} label={label}>
            <div className="search-row">
              <Input
                aria-label={label}
                value={String(draft[key])}
                placeholder={hint}
                onChange={(e) => setDraft({ ...draft, [key]: e.target.value })}
              />
              <Button
                variant="outline"
                disabled={busy}
                onClick={() => void choose(key)}
                aria-label={t('settings.storage.chooseRoot', { label: label })}
              >
                {t('common.actions.chooseDirectory')}
              </Button>
              <OpenPathButton path={String(draft[key])} />
            </div>
          </Field>
        ))}
        <p className="muted">
          {t(
            'settings.storage.layoutHint',
          )}
        </p>
        {(draft.tv_root || draft.movie_root || draft.music_root) && (
          <Disclosure>
            <DisclosureTrigger>{t('settings.storage.legacy.title')}</DisclosureTrigger>
            {(['tv_root', 'movie_root', 'music_root'] as const).map((key) => (
              <Field
                key={key}
                label={
                  {
                    tv_root: t('settings.storage.legacy.tv'),
                    movie_root: t('settings.storage.legacy.movie'),
                    music_root: t('settings.storage.legacy.music'),
                  }[key]
                }
              >
                <div className="search-row">
                  <Input
                    value={draft[key]}
                    onChange={(e) =>
                      setDraft({ ...draft, [key]: e.target.value })
                    }
                  />
                  <OpenPathButton path={draft[key]} />
                </div>
              </Field>
            ))}
          </Disclosure>
        )}
        <Separator className="section-separator" />
        <h2>{t('settings.naming.title')}</h2>
        <Tabs
          value={templateKind}
          onValueChange={(v) => setTemplateKind(v as typeof templateKind)}
        >
          <TabsList>
            <TabsTrigger value="movie">{t('common.media.movie')}</TabsTrigger>
            <TabsTrigger value="tv">{t('common.media.tv')}</TabsTrigger>
            <TabsTrigger value="music">{t('common.media.music')}</TabsTrigger>
          </TabsList>
        </Tabs>
        <Field label={t('settings.naming.workTemplate')}>
          <Textarea
            aria-label={t('settings.naming.template')}
            rows={5}
            value={draft.naming[templateKind]}
            onChange={(e) =>
              setDraft({
                ...draft,
                naming: { ...draft.naming, [templateKind]: e.target.value },
              })
            }
          />
        </Field>
        {templateError ? (
          <p role="alert" className="metadata-errors">
            {templateError}
          </p>
        ) : (
          <pre className="template-preview">{preview[templateKind]}</pre>
        )}
        <Disclosure>
          <DisclosureTrigger>{t('settings.naming.syntax')}</DisclosureTrigger>
          <p>
            {t(
              'settings.naming.syntaxHint',
            )}
          </p>
          <code className="template-vars">
            title · localized_title · original_title · year · tmdbid · season ·
            episode · season_episode · episode_title · original_episode_title ·
            fileExt · videoFormat · videoCodec · audioCodec · artist
          </code>
          <p>
            {t(
              'settings.naming.requiredVariables',
            )}
          </p>
        </Disclosure>
      </Card>
      <Card className="panel">
        <h2>{t('settings.sources.title')}</h2>
        <Field label={t('settings.language.label')}>
          <SelectField
            aria-label={t('settings.language.label')}
            value={getLanguagePreference()}
            onValueChange={(value) =>
              setLanguagePreference(value as LanguagePreference)
            }
          >
            <SelectOption value="system">{t('settings.language.system')}</SelectOption>
            <SelectOption value="zh-CN">简体中文</SelectOption>
            <SelectOption value="en">English</SelectOption>
          </SelectField>
          <p className="muted">
            {t('settings.language.hint')}
          </p>
        </Field>
        <Field label={t('settings.sources.tmdbLanguage')}>
          <Input
            aria-label={t('settings.sources.tmdbLanguage')}
            list="metadata-languages"
            value={draft.language}
            onChange={(e) => setDraft({ ...draft, language: e.target.value })}
          />
          <datalist id="metadata-languages">
            <option value="zh-CN">{t('common.languages.simplifiedChinese')}</option>
            <option value="zh-TW">{t('common.languages.traditionalChinese')}</option>
            <option value="zh-HK">{t('common.languages.hongKongChinese')}</option>
            <option value="ja-JP">{t('common.languages.japanese')}</option>
            <option value="en-US">English</option>
          </datalist>
        </Field>
        <p className="muted">{t('settings.sources.languageHint')}</p>
        <Field label={t('settings.subtitles.languagePattern.label')}>
          <Input
            aria-label={t('settings.subtitles.languagePattern.label')}
            aria-describedby="subtitle-pattern-help"
            value={draft.subtitle_language_pattern || ''}
            placeholder={defaultSubtitlePattern}
            onChange={(e) =>
              setDraft({ ...draft, subtitle_language_pattern: e.target.value })
            }
          />
          <p id="subtitle-pattern-help" className="muted">
            {t(
              'settings.subtitles.languagePattern.hint',
            )}
          </p>
        </Field>
        <Field label={t('settings.tools.directory.label')}>
          <div className="search-row">
            <Input aria-label={t('settings.tools.directory.label')}
              value={toolDirectoryInput}
              placeholder={toolDirectory === null ? t('settings.tools.directory.custom') : t('settings.tools.directory.placeholder')}
              onChange={e => setToolDirectory(e.target.value)} />
            <Button variant="outline" disabled={busy} onClick={() => void run(t('common.actions.chooseDirectory'), async () => {
              const path = await open({directory: true, multiple: false})
              if (typeof path === 'string') setToolDirectory(path)
            })}>{t('common.actions.chooseDirectory')}</Button>
            <OpenPathButton path={toolDirectory || ''} />
          </div>
          <p className="muted">{t('settings.tools.directory.hint')}</p>
        </Field>
        <Disclosure defaultOpen={toolDirectory === null}>
          <DisclosureTrigger>{t('settings.tools.individual')}</DisclosureTrigger>
          {(['ffmpeg', 'ffprobe'] as const).map(tool => (
            <Field key={tool} label={tool === 'ffmpeg' ? t('settings.tools.ffmpeg.label') : t('settings.tools.ffprobe.label')}>
              <div className="search-row">
                <Input aria-label={tool === 'ffmpeg' ? t('settings.tools.ffmpeg.label') : t('settings.tools.ffprobe.label')}
                  value={draft[tool] || ''} placeholder={tool}
                  onChange={e => setExecutable(tool, e.target.value)} />
                <Button variant="outline" disabled={busy}
                  onClick={() => void choose(tool, false)}>{t('common.actions.chooseFile')}</Button>
                <OpenPathButton path={draft[tool] || ''} disabled={!/[\\/]/.test(draft[tool] || '')} />
              </div>
            </Field>
          ))}
        </Disclosure>
        <div className="search-row">
          <Button variant="outline" disabled={busy} onClick={() => void run(t('settings.tools.ffmpeg.check'), async () => {
            const version = await call<string>('check_ffmpeg', {path: draft.ffmpeg || (toolDirectory ? audioToolPaths(toolDirectory).ffmpeg : '')})
            setToolVersion(version)
          })}>{t('settings.tools.ffmpeg.check')}</Button>
          {toolVersion && <small role="status">{toolVersion}</small>}
        </div>
        <Field label={t('settings.sources.tmdbToken')}>
          <Input
            type="password"
            autoComplete="off"
            value={draft.tmdb_token}
            onChange={(e) => setDraft({ ...draft, tmdb_token: e.target.value })}
            placeholder={t('settings.sources.tokenHint')}
          />
        </Field>
        <Field label={t('settings.sources.contactOptional')}>
          <Input
            aria-label={t('settings.sources.musicBrainzContact')}
            value={draft.musicbrainz_contact || ''}
            onChange={(e) =>
              setDraft({ ...draft, musicbrainz_contact: e.target.value })
            }
            placeholder={t('settings.sources.contactPlaceholder')}
          />
          <p className="muted">
            {t(
              'settings.sources.contactHint',
            )}
          </p>
        </Field>
        <Field label={t('settings.operations.default')}>
          <SelectField
            aria-label={t('settings.operations.default')}
            value={draft.default_operation}
            onValueChange={(value) =>
              setDraft({
                ...draft,
                default_operation: value as Settings['default_operation'],
              })
            }
          >
            <SelectOption value="copy">{t('common.operations.copy')}</SelectOption>
            <SelectOption value="move">{t('common.operations.move')}</SelectOption>
            <SelectOption value="hardlink">{t('common.operations.hardlink')}</SelectOption>
            <SelectOption value="symlink">{t('common.operations.symlink')}</SelectOption>
          </SelectField>
        </Field>
        <p className="muted">{t('settings.operations.hint')}</p>
        <Field label={t('settings.cue.originals.label')}>
          <SelectField aria-label={t('settings.cue.originals.label')}
            value={draft.cue_source_disposition || 'keep'}
            onValueChange={value => setDraft({...draft, cue_source_disposition: value as Settings['cue_source_disposition']})}>
            <SelectOption value="keep">{t('cue.originals.keep')}</SelectOption>
            <SelectOption value="delete">{t('cue.originals.delete')}</SelectOption>
            <SelectOption value="backup">{t('cue.originals.backup')}</SelectOption>
          </SelectField>
          <p className="muted">{t('settings.cue.originals.hint')}</p>
        </Field>
        <Label className="check-label">
          <Checkbox
            checked={draft.verify_hash}
            onCheckedChange={(v) =>
              setDraft({ ...draft, verify_hash: v === true })
            }
          />
          {t('settings.verification.fullHash')}
        </Label>
        <p className="muted">
          {t(
            'settings.verification.hint',
          )}
        </p>
        <Separator className="section-separator" />
        <h3>{t('settings.storage.localData')}</h3>
        <p className="path-text">
          {dataDir ? <FilePath path={dataDir} /> : t('settings.storage.createdOnLaunch')}
        </p>
        <h3>{t('settings.metadata.title')}</h3>
        <p className="muted">
          {t('settings.metadata.hint')}
        </p>
      </Card>
      <DiagnosticsPane />

    </div>
  )
}
