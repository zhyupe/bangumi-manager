import { FileIcon, GridIcon, ListBulletIcon } from '@radix-ui/react-icons'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import { FilePath } from '@/components/file-path'
import { Pages } from '@/components/pagination'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { call } from '@/lib/api'
import { bytes, natural } from '@/lib/format'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
import type { Signature } from '@/types'

interface ExtraFile {
  path: string
  relative_path: string
  signature: Signature
}
interface ExtraFilesResult {
  path: string
  files: ExtraFile[]
  errors: string[]
}
const imageFile = (path: string) => /\.(png|jpe?g|webp|gif|bmp)$/i.test(path)

export default function ExtraFiles({
  workId,
  revision,
}: {
  workId: string
  revision: string
}) {
  const [result, setResult] = useState<ExtraFilesResult>()
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [refresh, setRefresh] = useState(0)
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(1)
  const [view, setView] = useState(() =>
    localStorage.getItem('bangumi.extrasView') === 'list' ? 'list' : 'grid',
  )
  const [image, setImage] = useState<ExtraFile>()
  useEffect(() => {
    let active = true
    setLoading(true)
    setError('')
    setResult(undefined)
    setPage(1)
    void call<ExtraFilesResult>('list_work_extras', { workId })
      .then((value) => {
        if (active) setResult(value)
      })
      .catch((reason) => {
        if (active) setError(backendMessage(String(reason)))
      })
      .finally(() => {
        if (active) setLoading(false)
      })
    return () => {
      active = false
    }
  }, [workId, revision, refresh])
  const files = (result?.files || [])
    .filter((f) => f.relative_path.toLowerCase().includes(query.toLowerCase()))
    .sort((a, b) => natural(a.relative_path, b.relative_path))
  const shown = files.slice((page - 1) * 60, page * 60)
  const title = (file: ExtraFile) =>
    imageFile(file.path) ? (
      <Button
        variant="ghost"
        className="extra-file-name"
        onClick={() => setImage(file)}
      >
        {file.relative_path}
      </Button>
    ) : (
      <span className="extra-file-name">{file.relative_path}</span>
    )
  return (
    <section className="extra-files" aria-label={t('library.extras.files')}>
      <div className="selection-bar">
        <h3>{t('library.extras.files')}</h3>
        <Input
          aria-label={t('library.extras.filter')}
          placeholder={t('library.extras.filter')}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value)
            setPage(1)
          }}
        />
        <div
          className="button-row"
          role="group"
          aria-label={t('library.extras.view')}
        >
          {(['grid', 'list'] as const).map((mode) => (
            <Button
              key={mode}
              variant={view === mode ? 'secondary' : 'ghost'}
              aria-pressed={view === mode}
              onClick={() => {
                setView(mode)
                localStorage.setItem('bangumi.extrasView', mode)
              }}
            >
              {mode === 'grid' ? <GridIcon /> : <ListBulletIcon />}
              {mode === 'grid'
                ? t('library.extras.grid')
                : t('library.extras.list')}
            </Button>
          ))}
          <Button
            variant="outline"
            disabled={loading}
            onClick={() => setRefresh((n) => n + 1)}
          >
            {t('common.actions.refresh')}
          </Button>
        </div>
      </div>
      {result && <FilePath path={result.path} />}
      {loading && <p role="status">{t('library.extras.loading')}</p>}
      {error && <p role="alert">{error}</p>}
      {result?.errors.map((message) => (
        <p role="alert" key={message}>
          {backendMessage(message)}
        </p>
      ))}
      {!loading && result && !files.length && (
        <p className="muted">{t('library.extras.empty')}</p>
      )}
      {view === 'grid' ? (
        <div className="extra-file-grid">
          {shown.map((file) => (
            <article className="extra-file-tile" key={file.path}>
              {imageFile(file.path) ? (
                <button
                  className="extra-thumbnail"
                  aria-label={t('library.extras.preview', {
                    path: file.relative_path,
                  })}
                  onClick={() => setImage(file)}
                >
                  <img
                    src={convertFileSrc(file.path)}
                    alt=""
                    loading="lazy"
                    onError={(e) => {
                      e.currentTarget.hidden = true
                    }}
                  />
                  <FileIcon aria-hidden="true" />
                </button>
              ) : (
                <div className="extra-thumbnail">
                  <FileIcon aria-hidden="true" />
                </div>
              )}
              {title(file)}
              <span className="muted">{bytes(file.signature.size)}</span>
              <FilePath path={file.path} truncate />
            </article>
          ))}
        </div>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t('library.extras.files')}</TableHead>
              <TableHead>{t('library.extras.size')}</TableHead>
              <TableHead>{t('library.extras.path')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {shown.map((file) => (
              <TableRow key={file.path}>
                <TableCell>{title(file)}</TableCell>
                <TableCell>{bytes(file.signature.size)}</TableCell>
                <TableCell>
                  <FilePath path={file.path} />
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
      {files.length > 0 && (
        <Pages page={page} count={files.length} size={60} onChange={setPage} />
      )}
      <Dialog
        open={!!image}
        onOpenChange={(open) => {
          if (!open) setImage(undefined)
        }}
      >
        <DialogContent
          className="extra-image-dialog"
          aria-describedby={undefined}
        >
          <DialogTitle>{image?.relative_path}</DialogTitle>
          {image && (
            <img src={convertFileSrc(image.path)} alt={image.relative_path} />
          )}
        </DialogContent>
      </Dialog>
    </section>
  )
}
