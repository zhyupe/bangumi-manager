import { ReloadIcon } from '@radix-ui/react-icons'
import { useEffect, useState } from 'react'
import { Disclosure, DisclosureTrigger } from '@/components/disclosure'
import { FilePath } from '@/components/file-path'
import { Pages } from '@/components/pagination'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { getLocale, t } from '@/i18n'
import './diagnostics.css'
import { backendMessage } from '@/i18n/backend'
import { call } from '@/lib/api'

interface TimingEvent {
  timestamp: string
  trace_id: string
  span_id?: string
  parent_id?: string | null
  operation: string
  status: string
  elapsed_ms: number
  fields: Record<string, unknown>
}
interface TimingReport {
  directory: string
  session_id: string
  write_error: string | null
  statistics: {
    operation: string
    count: number
    failures: number
    total_ms: number
    max_ms: number
    last_ms: number
  }[]
  recent: TimingEvent[]
  active: TimingEvent[]
}
export const duration = (ms: number) =>
  ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${ms.toFixed(1)} ms`
const statusText = (status: string) =>
  ({
    ok: t('common.status.complete'),
    error: t('common.status.failed'),
    cancelled: t('diagnostics.status.cancelled'),
    interrupted: t('diagnostics.status.interrupted'),
    running: t('common.status.running'),
  })[status] || status
function TimingRow({ event }: { event: TimingEvent }) {
  const timestamp = new Date(event.timestamp)
  const formattedTime = Number.isNaN(timestamp.getTime())
    ? event.timestamp
    : timestamp.toLocaleString(getLocale(), { hour12: false })
  const fields = Object.entries(event.fields)
  return (
    <Disclosure className="diagnostics-event">
      <DisclosureTrigger className="diagnostics-event-trigger">
        <span className="diagnostics-operation">{event.operation}</span>
        <span className="diagnostics-duration">
          {duration(event.elapsed_ms)}
        </span>
        <Badge
          variant="outline"
          tone={
            event.status === 'error'
              ? 'red'
              : event.status === 'running'
                ? 'blue'
                : 'gray'
          }
        >
          {statusText(event.status)}
        </Badge>
      </DisclosureTrigger>
      <div className="diagnostics-detail">
        <dl className="diagnostics-metadata">
          <div>
            <dt>{t('diagnostics.fields.timestamp')}</dt>
            <dd>
              <time dateTime={event.timestamp}>{formattedTime}</time>
            </dd>
          </div>
          <div>
            <dt>{t('diagnostics.fields.requestId')}</dt>
            <dd>
              <code>{event.trace_id}</code>
            </dd>
          </div>
          {event.span_id && (
            <div>
              <dt>{t('diagnostics.fields.operationId')}</dt>
              <dd>
                <code>{event.span_id}</code>
              </dd>
            </div>
          )}
          {event.parent_id && (
            <div>
              <dt>{t('diagnostics.fields.parentOperation')}</dt>
              <dd>
                <code>{event.parent_id}</code>
              </dd>
            </div>
          )}
        </dl>
        {fields.length > 0 && (
          <dl className="diagnostics-fields">
            {fields.map(([name, value]) => (
              <div key={name}>
                <dt>{name}</dt>
                <dd>
                  <pre>
                    {typeof value === 'string'
                      ? value
                      : JSON.stringify(value, null, 2)}
                  </pre>
                </dd>
              </div>
            ))}
          </dl>
        )}
      </div>
    </Disclosure>
  )
}
function EventColumns() {
  return (
    <div className="diagnostics-columns" aria-hidden="true">
      <span />
      <span>{t('diagnostics.columns.operation')}</span>
      <span>{t('diagnostics.fields.duration')}</span>
      <span>{t('common.fields.status')}</span>
    </div>
  )
}
const pageSize = 20
export default function DiagnosticsPane() {
  const [report, setReport] = useState<TimingReport>()
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [page, setPage] = useState(1)
  async function refresh() {
    setLoading(true)
    try {
      setReport(await call<TimingReport>('diagnostics_report'))
      setPage(1)
      setError('')
    } catch (e) {
      setError(backendMessage(e))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => {
    void refresh()
  }, [])
  return (
    <Card className="panel diagnostics-panel" aria-busy={loading}>
      <div className="diagnostics-header">
        <div>
          <h2>{t('diagnostics.title')}</h2>
          <p className="diagnostics-description">
            {t('diagnostics.description')}
          </p>
        </div>
        <div className="button-row">
          <Button
            variant="secondary"
            disabled={loading}
            onClick={() => void refresh()}
          >
            <ReloadIcon aria-hidden="true" />
            {t('diagnostics.actions.refresh')}
          </Button>
          <Button
            variant="outline"
            onClick={() =>
              void call('open_log_directory').catch((e) =>
                setError(backendMessage(e)),
              )
            }
          >
            {t('diagnostics.actions.openDirectory')}
          </Button>
        </div>
      </div>
      {error && (
        <p className="diagnostics-error" role="alert">
          {error}
        </p>
      )}
      {report?.write_error && (
        <p className="diagnostics-error" role="alert">
          {backendMessage(report.write_error)}
        </p>
      )}
      {loading && (
        <p className="diagnostics-empty" role="status">
          {t('diagnostics.loading')}
        </p>
      )}
      {report && (
        <>
          <div className="diagnostics-location">
            <span>{t('diagnostics.directory')}</span>
            <FilePath path={report.directory} openButton={false} />
          </div>
          <Disclosure className="diagnostics-guide">
            <DisclosureTrigger>{t('diagnostics.help.title')}</DisclosureTrigger>
            <p>
              {t(
                'diagnostics.retentionHint',
              )}
            </p>
            <p>
              {t(
                'diagnostics.help.operations',
              )}
            </p>
            <p>
              {t(
                'diagnostics.help.scanScope',
              )}
            </p>
          </Disclosure>
          <section
            className="diagnostics-section diagnostics-active"
            aria-label={t('diagnostics.active.title')}
          >
            <div className="diagnostics-section-heading">
              <h3>
                {t('diagnostics.active.title')}{' '}
                <span className="diagnostics-count">
                  {report.active.length}
                </span>
              </h3>
              <span>{t('diagnostics.active.refreshHint')}</span>
            </div>
            {report.active.length === 0 ? (
              <p className="diagnostics-empty">{t('diagnostics.active.empty')}</p>
            ) : (
              <>
                <EventColumns />
                {report.active.map((event) => (
                  <TimingRow
                    key={
                      event.span_id ||
                      `${event.trace_id}:${event.operation}:${event.timestamp}`
                    }
                    event={event}
                  />
                ))}
              </>
            )}
          </section>
          <Disclosure className="diagnostics-section">
            <DisclosureTrigger className="diagnostics-section-trigger">
              {t('diagnostics.statistics.count', {
                count: report.statistics.length,
              })}
            </DisclosureTrigger>
            {report.statistics.length === 0 ? (
              <p className="diagnostics-empty">{t('diagnostics.empty')}</p>
            ) : (
              <Table className="diagnostics-statistics">
                <TableHeader>
                  <TableRow>
                    <TableHead>{t('diagnostics.columns.operation')}</TableHead>
                    <TableHead>{t('diagnostics.columns.attempts')}</TableHead>
                    <TableHead>{t('diagnostics.columns.average')}</TableHead>
                    <TableHead>{t('diagnostics.columns.maximum')}</TableHead>
                    <TableHead>{t('diagnostics.columns.latest')}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {report.statistics.map((s) => (
                    <TableRow key={s.operation}>
                      <TableCell>{s.operation}</TableCell>
                      <TableCell>
                        {s.count} / {s.failures}
                      </TableCell>
                      <TableCell>
                        {duration(s.count ? s.total_ms / s.count : 0)}
                      </TableCell>
                      <TableCell>{duration(s.max_ms)}</TableCell>
                      <TableCell>{duration(s.last_ms)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </Disclosure>
          <Disclosure className="diagnostics-section" defaultOpen>
            <DisclosureTrigger className="diagnostics-section-trigger">
              {t('diagnostics.recent.count', {
                count: report.recent.length,
              })}
            </DisclosureTrigger>
            {report.recent.length === 0 ? (
              <p className="diagnostics-empty">{t('diagnostics.empty')}</p>
            ) : (
              <>
                <EventColumns />
                {report.recent
                  .slice((page - 1) * pageSize, page * pageSize)
                  .map((event) => (
                    <TimingRow
                      key={
                        event.span_id ||
                        `${event.trace_id}:${event.operation}:${event.timestamp}`
                      }
                      event={event}
                    />
                  ))}
                {report.recent.length > pageSize && (
                  <Pages
                    page={page}
                    count={report.recent.length}
                    size={pageSize}
                    onChange={setPage}
                  />
                )}
              </>
            )}
          </Disclosure>
        </>
      )}
    </Card>
  )
}
