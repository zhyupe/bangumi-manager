import { beforeEach, describe, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mock.invoke,
  isTauri: () => true,
}))

import { call, measureAction } from './api'

beforeEach(() => {
  mock.invoke.mockReset()
})
describe('operation timing', () => {
  it('correlates an IPC roundtrip without logging arguments or response', async () => {
    mock.invoke
      .mockResolvedValueOnce({ token: 'private-result' })
      .mockResolvedValue(undefined)
    expect(
      await call('search_media', { query: 'private-query', token: 'secret' }),
    ).toEqual({ token: 'private-result' })
    const request = mock.invoke.mock.calls[0][1]
    expect(request.traceId).toMatch(/^[a-f0-9-]{36}$/)
    const [command, event] = mock.invoke.mock.calls[1]
    expect(command).toBe('record_frontend_timing')
    expect(event).toMatchObject({
      operation: 'ipc.search_media',
      traceId: request.traceId,
      status: 'ok',
    })
    expect(event.elapsedMs).toBeGreaterThanOrEqual(0)
    expect(JSON.stringify(event)).not.toMatch(/private|secret/)
  })
  it('preserves the operation error even when logging fails', async () => {
    const error = new Error('original error')
    mock.invoke
      .mockRejectedValueOnce(error)
      .mockRejectedValue(new Error('logging unavailable'))
    await expect(call('media_detail')).rejects.toBe(error)
    expect(mock.invoke.mock.calls[1][1].status).toBe('error')
    await Promise.resolve()
  })
  it('does not recursively measure the diagnostics commands', async () => {
    mock.invoke.mockResolvedValue({ recent: [] })
    await call('diagnostics_report')
    expect(mock.invoke).toHaveBeenCalledTimes(1)
  })
  it('records user action failure without swallowing it', async () => {
    mock.invoke.mockResolvedValue(undefined)
    await expect(
      measureAction('加载条目与集数', async () => {
        throw new Error('failed')
      }),
    ).rejects.toThrow('failed')
    expect(mock.invoke.mock.calls[0][1]).toMatchObject({
      operation: 'action.加载条目与集数',
      status: 'error',
    })
  })
})
