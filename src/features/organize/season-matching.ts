import { isSubtitle } from '@/lib/subtitles'
import type { Mapping, SourceFile, Work } from '@/types'

export function seasonMatchSummary(
  work: Work | undefined,
  files: SourceFile[],
  mappings: Record<string, Mapping>,
  season: number,
  binding: string,
  subtitles = false,
) {
  const episodes = (work?.episodes || []).filter(
    (ep) =>
      ep.enabled &&
      ep.number > 0 &&
      ep.season === season &&
      (!binding || ep.binding_id === binding),
  )
  const numbersById = new Map(episodes.map((ep) => [ep.id, ep.number]))
  const expected = [...new Set(numbersById.values())].sort((a, b) => a - b)
  const matched = new Set<number>()
  for (const file of files) {
    const mapping = mappings[file.path]
    if (
      !(subtitles ? isSubtitle(file.name) : file.media) ||
      !mapping ||
      mapping.extra ||
      mapping.work_id !== work?.id
    )
      continue
    for (const id of mapping.episode_ids) {
      const number = numbersById.get(id)
      if (number !== undefined) matched.add(number)
    }
  }
  return {
    total: expected.length,
    matched: matched.size,
    missing: expected.filter((n) => !matched.has(n)),
  }
}

export function episodeRanges(numbers: number[], prefix = 'EP') {
  const ranges: [number, number][] = []
  for (const number of [...new Set(numbers)].sort((a, b) => a - b)) {
    const previous = ranges.at(-1)
    if (previous && previous[1] + 1 === number) previous[1] = number
    else ranges.push([number, number])
  }
  const label = (n: number) => `${prefix}${String(n).padStart(2, '0')}`
  return ranges
    .map(([start, end]) =>
      start === end ? label(start) : `${label(start)}–${label(end)}`,
    )
    .join('、')
}
