import { describe, expect, it } from 'vitest'
import type { SourceFile } from '@/types'
import {
  commonKeywords,
  preferredProvider,
  suggestSourceImport,
} from './source-search'

function file(name: string, parent = 'D:\\Incoming', media = true): SourceFile {
  return {
    name,
    parent,
    path: `${parent}\\${name}`,
    extension: name.split('.').at(-1)!,
    media,
    hint: 'episode',
    signature: { size: 1, modified: 1 },
    probe: null,
  }
}

describe('search suggestions from selected sources', () => {
  it('removes episode and codec tags separated by underscores in a single selected file', () => {
    expect(
      suggestSourceImport(
        [file('Fictional_Zephyr_Atlas_S02E01_1080p_H_264.mkv')],
        'movie',
      ),
    ).toEqual({ kind: 'tv', keyword: 'Fictional Zephyr Atlas' })
  })
  it('keeps shared title words in filename order and removes technical episode tags', () => {
    const files = [
      file('Fictional.Zephyr.Atlas.S02E01.1080p.H.264.mkv'),
      file('fictional.zephyr.atlas.S02E02.720p.HEVC.mkv'),
    ]
    expect(suggestSourceImport(files)).toEqual({
      kind: 'tv',
      keyword: 'Fictional Zephyr Atlas',
    })
    expect(commonKeywords(files).map((w) => w.count)).toEqual([2, 2, 2])
  })
  it('cleans a single movie filename without including its generic source directory', () => {
    expect(
      suggestSourceImport(
        [file('虚构星桥.2025.2160p.WEB-DL.H.265.DDP5.1.mkv')],
        'movie',
      ),
    ).toEqual({ kind: 'movie', keyword: '虚构星桥' })
  })
  it('uses the shared album directory instead of individual song names', () => {
    const parent = 'D:\\Music\\虚构回响\\Disc 02'
    expect(
      suggestSourceImport([
        file('01 - 虚构音轨甲.flac', parent),
        file('02 - 虚构音轨乙.flac', parent),
      ]),
    ).toEqual({ kind: 'music', keyword: '虚构回响' })
  })
  it('falls back to a meaningful parent for numeric episode filenames', () => {
    expect(
      suggestSourceImport([
        file('01.mkv', 'D:\\Incoming\\虚构远航\\Season 1'),
        file('02.mkv', 'D:\\Incoming\\虚构远航\\Season 1'),
      ]).keyword,
    ).toBe('虚构远航')
  })
  it('does not use download folders or unmatched majority words as the automatic query', () => {
    expect(
      suggestSourceImport([file('虚构甲.mkv'), file('虚构乙.mkv')]).keyword,
    ).toBe('')
    expect(
      suggestSourceImport([
        file('虚构甲 01.mkv'),
        file('虚构甲 02.mkv'),
        file('虚构乙 03.mkv'),
      ]).keyword,
    ).toBe('')
    expect(suggestSourceImport([file('1080p.H264.mkv')]).keyword).toBe('')
  })
  it('ignores subtitle and NFO files when media files are selected', () => {
    expect(
      suggestSourceImport([
        file('虚构远航 S01E01.mkv'),
        file('metadata.nfo', 'D:\\Incoming', false),
        file('other.ass', 'D:\\Incoming', false),
      ]).keyword,
    ).toBe('虚构远航')
  })
  it('does not mistake UNC server and share names for a title', () => {
    expect(
      suggestSourceImport([
        file('01.mkv', '\\\\server\\library\\Incoming'),
        file('02.mkv', '\\\\server\\library\\Incoming'),
      ]).keyword,
    ).toBe('')
  })
  it('supports parent hints from older source records without a parent field', () => {
    const source = file('01.flac', 'D:\\Music\\虚构专辑')
    source.parent = ''
    expect(suggestSourceImport([source]).keyword).toBe('虚构专辑')
  })
  it('infers audio selection independently of the currently selected work', () => {
    expect(
      suggestSourceImport(
        [file('虚构专辑 01.flac'), file('虚构专辑 02.MP3')],
        'tv',
      ).kind,
    ).toBe('music')
    expect(suggestSourceImport([file('虚构影片.mkv')], 'music').kind).toBe('tv')
    expect(
      suggestSourceImport(
        [file('虚构影片.mkv'), file('虚构音轨.flac')],
        'music',
      ).kind,
    ).toBe('tv')
    expect(
      suggestSourceImport([file('虚构远航 S02E01.mkv')], 'movie').kind,
    ).toBe('tv')
  })
  it('keeps an empty selection editable and preserves its media context', () => {
    expect(suggestSourceImport([])).toEqual({ kind: 'tv', keyword: '' })
    expect(suggestSourceImport([], 'movie')).toEqual({
      kind: 'movie',
      keyword: '',
    })
    expect(suggestSourceImport([], 'music')).toEqual({
      kind: 'music',
      keyword: '',
    })
  })
  it('prefers TMDB for non-music and the supported music provider for albums', () => {
    expect(preferredProvider('tv')).toBe('tmdb')
    expect(preferredProvider('movie')).toBe('tmdb')
    expect(preferredProvider('music')).toBe('bangumi')
  })
})
