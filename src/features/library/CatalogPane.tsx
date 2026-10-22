import {
  MagnifyingGlassIcon,
  PlusIcon,
  ReloadIcon,
} from '@radix-ui/react-icons'
import { useEffect, useState } from 'react'
import { Disclosure, DisclosureTrigger } from '@/components/disclosure'
import { Empty } from '@/components/empty-state'
import { FilePath } from '@/components/file-path'
import { SelectField, SelectOption } from '@/components/select-field'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import type { ImportTarget } from '@/features/metadata/MediaImportDialog'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import { call } from '@/lib/api'
import { artwork } from '@/lib/artwork'
import { natural } from '@/lib/format'
import type { LibraryIndex, Plan, ReplacementTarget, Run, Work } from '@/types'
import { catalogCollection } from './catalog'
import type { LibraryDrop, SubmitMode } from './import-model'
import { SeasonCollectionSummary } from './SeasonCollection'
import WorkDetail from './WorkDetail'

interface Props {
  detailRequest?: string
  onDetailOpened?: () => void
  onReplace: (target: ReplacementTarget) => void
  onRename: (paths: string[]) => void
  kind: string
  works: Work[]
  plans: Plan[]
  onDelete: (id: string) => Promise<void>
  workId: string
  onWork: (id: string) => void
  index: LibraryIndex
  busy: boolean
  importBusy?: boolean
  run: Run
  onReload: () => Promise<void>
  onPlan: (p: Plan, mode: SubmitMode) => Promise<void>
  dropRequest: LibraryDrop | null
  onDropHandled: () => void
  onAdd: (target: ImportTarget) => void
}
export default function CatalogPane(props: Props) {
  const { kind, works, index, busy, run, onReload, onWork, onAdd } = props
  const [query, setQuery] = useState(''),
    [detail, setDetail] = useState('')
  const [bindings, setBindings] = useState<Record<string, string>>({})
  useEffect(() => {
    setQuery('')
    setDetail('')
  }, [kind])
  useEffect(() => {
    if (props.detailRequest) {
      setDetail(props.detailRequest)
      props.onDetailOpened?.()
    }
  }, [props.detailRequest, props.onDetailOpened])
  const shown = works
    .filter(
      (w) =>
        w.kind === kind &&
        `${w.title} ${w.info?.localized_title || ''} ${w.info?.original_title || ''}`
          .toLocaleLowerCase()
          .includes(query.toLocaleLowerCase()),
    )
    .sort((a, b) => natural(a.title, b.title))
  if (detail && works.some((w) => w.id === detail))
    return (
      <WorkDetail
        key={detail}
        {...props}
        workId={detail}
        onBack={() => setDetail('')}
      />
    )
  return (
    <section className="catalog-browser">
      <div className="catalog-search">
        <div className="catalog-search-input">
          <MagnifyingGlassIcon />
          <Input
            aria-label={t('library.search.label')}
            placeholder={t('library.search.placeholder')}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <Button
          variant="secondary"
          disabled={busy}
          onClick={() =>
            void run(t('library.scan.action'), async () => {
              await call('refresh_library')
              await onReload()
            })
          }
        >
          <ReloadIcon />
          {t('common.actions.scan')}
        </Button>
        <Button
          disabled={props.importBusy ?? busy}
          onClick={() => onAdd({ kind })}
        >
          <PlusIcon />
          {t('common.actions.add')}
        </Button>
      </div>
      <div className="library-state">
        <span>{t('library.summary.workCount', { count: shown.length })}</span>
        {!!index.imported && (
          <span>
            {t('library.scan.importedCount', {
              count: index.imported,
            })}
          </span>
        )}
        {!!index.updated && (
          <span>
            {t('library.scan.updatedCount', { count: index.updated })}
          </span>
        )}
        {index.scanned_at && (
          <Badge tone={index.available ? 'gray' : 'amber'}>
            {index.available
              ? t('library.scan.complete')
              : t('library.scan.offline')}
          </Badge>
        )}
      </div>
      {!!index.errors.length && (
        <Disclosure className="metadata-errors">
          <DisclosureTrigger>{t('library.scan.details')}</DisclosureTrigger>
          {index.errors.map((e, i) => (
            <p key={i}>{backendMessage(e)}</p>
          ))}
        </Disclosure>
      )}
      {shown.length ? (
        <div className={`work-grid ${kind === 'music' ? 'album-grid' : ''}`}>
          {shown.map((w) => {
            const assets = index.assets.filter((a) => a.work_id === w.id)
            const collection =
              w.kind === 'tv' ? catalogCollection(w, assets) : null
            const scan = !index.scanned_at
              ? 'unscanned'
              : index.available
                ? 'current'
                : 'offline'
            return (
              <Button
                className="work-card"
                key={w.id}
                onClick={() => {
                  onWork(w.id)
                  setDetail(w.id)
                }}
                variant="ghost"
              >
                <div className="work-cover">
                  {artwork(w) ? (
                    <img src={artwork(w)} alt="" loading="lazy" />
                  ) : (
                    <span>{w.title.slice(0, 2)}</span>
                  )}
                  {collection ? (
                    <SeasonCollectionSummary
                      collection={collection}
                      scan={scan}
                    />
                  ) : (
                    <Badge tone={assets.length ? 'green' : 'gray'}>
                      {assets.length
                        ? t('library.resources.count', { count: assets.length })
                        : t('library.resources.empty')}
                    </Badge>
                  )}
                </div>
                <strong className="work-card-title" title={w.title}>
                  {w.title}
                </strong>
                <small className="work-card-translation">
                  {w.info?.localized_title !== w.title &&
                    w.info?.localized_title}
                </small>
                <small className="work-card-meta">
                  {w.year || t('common.values.unknownYear')}
                  {w.category ? ` · ${w.category}` : ''}
                </small>
              </Button>
            )
          })}
        </div>
      ) : (
        <Empty title={query ? t('library.empty.noResults') : t('library.empty.noWorks')}>
          <Button variant="secondary" onClick={() => onAdd({ kind })}>
            {t('library.actions.addFromDatabase')}
          </Button>
        </Empty>
      )}
      {!!index.unbound.length && (
        <Disclosure className="unbound">
          <DisclosureTrigger>
            {t('library.unbound.count', { count: index.unbound.length })}
          </DisclosureTrigger>
          {index.unbound.map((f) => (
            <div className="unbound-row" key={f.path}>
              <div>
                <strong>{f.name}</strong>
                <FilePath path={f.path} />
                {f.reason && (
                  <p className="error-text">{backendMessage(f.reason)}</p>
                )}
              </div>
              <SelectField
                aria-label={t('library.unbound.workLabel', { directory: f.name })}
                value={bindings[f.path] || ''}
                onValueChange={(value) =>
                  setBindings({ ...bindings, [f.path]: value })
                }
              >
                <SelectOption value="">{t('common.actions.chooseWork')}</SelectOption>
                {works
                  .filter((w) => w.kind === f.kind)
                  .map((w) => (
                    <SelectOption key={w.id} value={w.id}>
                      {w.title}
                    </SelectOption>
                  ))}
              </SelectField>
              <Button
                disabled={busy || !bindings[f.path]}
                onClick={() =>
                  void run(t('library.unbound.linkDirectory'), async () => {
                    await call('bind_library_folder', {
                      path: f.path,
                      workId: bindings[f.path],
                    })
                    await call('refresh_library')
                    await onReload()
                  })
                }
              >
                {t('common.actions.link')}
              </Button>
            </div>
          ))}
        </Disclosure>
      )}
    </section>
  )
}
