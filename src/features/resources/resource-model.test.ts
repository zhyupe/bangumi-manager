import { describe, expect, it } from 'vitest'
import type { ResourceFile } from '@/types'
import {
  defaultFilters,
  filterResources,
  resourceStatus,
  sortResources,
} from './resource-model'

const file = (
  name: string,
  size: number,
  height: number | null,
  bitrate: number | null,
): ResourceFile => ({
  path: `D:\\TV\\${name}`,
  name,
  extension: name.split('.').pop()!,
  kind: 'tv',
  signature: { size, modified: 1 },
  asset: null,
  available: true,
  probe: {
    width: height ? (height * 16) / 9 : null,
    height,
    bitrate,
    duration: 1440,
    video_codec: 'hevc',
    audio: ['aac'],
    error: null,
  },
})
describe('resource discovery', () => {
  it('sorts numeric values and puts unknown values last in both directions', () => {
    const files = [
      file('a.mkv', 100, 1080, 12e6),
      file('b.mkv', 20, null, null),
      file('c.mkv', 9, 720, 2e6),
    ]
    expect(
      sortResources(files, 'size', true).map((f) => f.signature.size),
    ).toEqual([100, 20, 9])
    expect(sortResources(files, 'bitrate', false).map((f) => f.name)).toEqual([
      'c.mkv',
      'a.mkv',
      'b.mkv',
    ])
    expect(sortResources(files, 'resolution', true).map((f) => f.name)).toEqual(
      ['a.mkv', 'c.mkv', 'b.mkv'],
    )
    expect(files[0].name).toBe('a.mkv')
  })
  it('combines extension, path search, height, bitrate and size ranges', () => {
    const files = [
      file('Show1.ts', 3 * 1024 ** 3, 720, 20e6),
      file('Show2.mkv', 1024 ** 3, 1080, 6e6),
      file('Show3.m2ts', 4 * 1024 ** 3, 720, 25e6),
    ]
    const filters = {
      ...defaultFilters,
      query: 'TV',
      extension: '.TS,m2ts',
      minSize: '2',
      maxSize: '3.5',
      maxHeight: '720',
      minBitrate: '10',
    }
    expect(filterResources(files, [], filters).map((f) => f.name)).toEqual([
      'Show1.ts',
    ])
  })
  it('does not treat missing media values as zero', () => {
    const files = [file('unknown.ts', 10, null, null)]
    expect(
      filterResources(files, [], { ...defaultFilters, maxBitrate: '10' }),
    ).toHaveLength(0)
    expect(
      filterResources(files, [], { ...defaultFilters, status: 'unknown' }),
    ).toHaveLength(1)
  })
  it('distinguishes offline, unprobed and failed files', () => {
    const f = file('file.mkv', 10, 1080, 3e6)
    expect(resourceStatus({ ...f, available: false })).toBe('offline')
    expect(resourceStatus({ ...f, probe: null })).toBe('missing')
    expect(resourceStatus({ ...f, unavailable: true })).toBe('unavailable')
    expect(
      resourceStatus({
        ...f,
        probe: { ...f.probe!, error: 'ffprobe missing' },
      }),
    ).toBe('error')
  })
  it('filters the whole collection before pagination', () => {
    const files = Array.from({ length: 163 }, (_, i) =>
      file(`${i}.ts`, i * 100, 720, 2e6),
    )
    const result = sortResources(
      filterResources(files, [], { ...defaultFilters, extension: 'ts' }),
      'size',
      true,
    )
    expect(result).toHaveLength(163)
    expect(result.slice(80, 160)).toHaveLength(80)
    expect(result.at(-1)?.signature.size).toBe(0)
  })
})
