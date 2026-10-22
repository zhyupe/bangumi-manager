import { invoke, isTauri } from '@tauri-apps/api/core'
import { t } from '@/i18n'
import { backendMessage } from '@/i18n/backend'
export const desktop = isTauri()
const diagnosticsCommands = new Set([
  'record_frontend_timing',
  'diagnostics_report',
  'open_log_directory',
])
function record(
  operation: string,
  traceId: string,
  started: number,
  status: string,
) {
  if (!desktop) return
  void invoke('record_frontend_timing', {
    operation,
    traceId,
    elapsedMs: performance.now() - started,
    status,
  }).catch(() => {})
}
export async function measureAction<T>(
  label: string,
  fn: () => Promise<T>,
): Promise<T> {
  const started = performance.now(),
    traceId = crypto.randomUUID()
  let status = 'ok'
  try {
    return await fn()
  } catch (e) {
    status = 'error'
    throw e
  } finally {
    record(`action.${label}`, traceId, started, status)
  }
}
export async function call<T>(
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (!desktop)
    return Promise.reject(
      new Error(t('app.preview.desktopRequired')),
    )
  if (diagnosticsCommands.has(command)) return invoke<T>(command, args)
  const started = performance.now(),
    traceId = crypto.randomUUID()
  let status = 'ok'
  try {
    return await invoke<T>(command, { ...args, traceId })
  } catch (e) {
    status = 'error'
    const message = backendMessage(e)
    if (message === (e instanceof Error ? e.message : String(e))) throw e
    throw new Error(message, { cause: e })
  } finally {
    record(`ipc.${command}`, traceId, started, status)
  }
}
