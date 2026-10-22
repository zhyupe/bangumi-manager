import { useEffect, useRef, useState } from 'react'
import { t } from '@/i18n'
import { call } from '@/lib/api'
import { inferParts } from '@/lib/media-parts'
import type { Mapping, Plan, Run, SourceFile } from '@/types'
import { mergeImports, type SubmitMode } from './import-model'

const discard = (id: string) =>
  call('discard_organize_preview', { id }).catch(() => {})

export function useLibraryImport(
  workId: string,
  run: Run,
  onPlan: (plan: Plan, mode: SubmitMode) => Promise<void>,
  kind = 'tv',
) {
  const [preview, setPreview] = useState<Plan | null>(null)
  const [error, setError] = useState('')
  const [scanErrors, setScanErrors] = useState<string[]>([])
  const mappings = useRef<Mapping[]>([])
  const currentId = useRef('')
  const generation = useRef(0)
  useEffect(() => {
    mappings.current = []
    currentId.current = ''
    setPreview(null)
    setError('')
    setScanErrors([])
    return () => {
      generation.current++
      if (currentId.current) void discard(currentId.current)
    }
  }, [workId])

  const build = async (next: Mapping[], version: number) => {
    const result = await call<Plan>('preview_organize', { mappings: next })
    if (version !== generation.current) {
      void discard(result.id)
      return
    }
    const previous = currentId.current
    currentId.current = result.id
    mappings.current = next
    setPreview(result)
    if (previous) void discard(previous)
  }
  const perform = (
    label: string,
    operation: (version: number) => Promise<void>,
  ) => {
    const version = generation.current
    return run(label, async () => {
      setError('')
      try {
        await operation(version)
      } catch (e) {
        if (version === generation.current) setError(String(e))
      }
    })
  }
  const stage = (paths: string[], episodeId?: string) =>
    perform(t('library.import.preview'), async (version) => {
      let incoming: Mapping[]
      if (episodeId) {
        const files = paths.map((path) => ({
          path,
          name: path.replace(/\\/g, '/').split('/').at(-1)!,
          parent: path.replace(/[/\\][^/\\]+$/, ''),
        }))
        const parts = inferParts(files, kind)
        if (
          paths.length !== 1 &&
          (parts.size !== paths.length ||
            new Set(parts.values()).size !== paths.length)
        )
          throw new Error(
            t(
              'library.import.partsHint',
            ),
          )
        incoming = files.map((file) => ({
          source: file.path,
          ...(parts.has(file.path) ? { part: parts.get(file.path)! } : {}),
          work_id: workId,
          episode_ids: [episodeId],
          extra: false,
        }))
      } else {
        const result = await call<{ files: SourceFile[]; errors: string[] }>(
          'scan_files',
          { paths },
        )
        if (version !== generation.current) return
        setScanErrors(result.errors)
        if (!result.files.length)
          throw new Error(t('library.extras.noFiles'))
        incoming = result.files.map((file) => ({
          source: file.path,
          work_id: workId,
          episode_ids: [],
          extra: true,
        }))
      }
      await build(mergeImports(mappings.current, incoming), version)
    })
  const clear = () => {
    generation.current++
    if (currentId.current) void discard(currentId.current)
    currentId.current = ''
    mappings.current = []
    setPreview(null)
    setError('')
    setScanErrors([])
  }
  const remove = (source: string) =>
    perform(t('library.import.refreshPreview'), async (version) => {
      const next = mappings.current.filter((m) => m.source !== source)
      if (!next.length) clear()
      else await build(next, version)
    })
  const refresh = () =>
    perform(t('library.import.refreshPreview'), (version) => build(mappings.current, version))
  const submit = (mode: SubmitMode) =>
    perform(t('library.import.submit'), async () => {
      if (!preview) return
      const plan = await call<Plan>('submit_organize', {
        id: preview.id,
        choices: preview.rows
          .filter((r) => !r.parent_id)
          .map((r) => ({
            id: r.id,
            action: r.action,
            operation: r.operation || 'copy',
            reviewed: r.reviewed,
          })),
      })
      currentId.current = ''
      mappings.current = []
      setPreview(null)
      setScanErrors([])
      await onPlan(plan, mode)
    })
  return {
    preview,
    setPreview,
    error,
    scanErrors,
    stage,
    remove,
    clear,
    refresh,
    submit,
  }
}
