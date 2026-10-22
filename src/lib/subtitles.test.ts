import { afterEach, expect, test } from 'vitest'
import { configureSubtitlePattern, subtitleName } from './subtitles'
import { numbers } from '@/features/organize/matching'

afterEach(() => configureSubtitlePattern())

test('normalizes only the final language segment', () => {
  expect(subtitleName('Chapter_sc_tag.E01.sc_xxxxxx.ass')).toEqual({
    stem: 'Chapter_sc_tag.E01',
    suffix: '.sc.ass',
  })
  expect(subtitleName('Chapter.E01.chs&jpn.ass').suffix).toBe('.chs&jpn.ass')
  expect(subtitleName('Chapter.E01.unknown_tag.ass').stem).toBe(
    'Chapter.E01.unknown_tag',
  )
  expect(subtitleName('Chapter.E01.sc_tag.extra.ass').stem).toBe(
    'Chapter.E01.sc_tag.extra',
  )
  expect(subtitleName('Chapter.E01.sc_123.ass').stem).toBe('Chapter.E01')
})

test('custom capture controls matching and strips the entire original segment', () => {
  configureSubtitlePattern('subtitle-([a-z]+)_[0-9]+')
  expect(subtitleName('Chapter.subtitle-CHS_123.ass')).toEqual({
    stem: 'Chapter',
    suffix: '.chs.ass',
  })
  expect(numbers('Chapter.E01.subtitle-chs_123.ass')).toEqual([1])
  expect(subtitleName('Chapter.sc_tag.ass').stem).toBe('Chapter.sc_tag')
})

test('does not emit unsafe, empty or partial language captures', () => {
  for (const pattern of ['(.*)', '(sc)', '(', 'sc_.*', '()sc_.*']) {
    expect(subtitleName('Chapter.sc_../bad.ass', pattern).suffix).not.toContain(
      '/',
    )
  }
  expect(subtitleName('Chapter.sc_tag.ass', '(sc)').stem).toBe('Chapter.sc_tag')
})
