import { natural } from '@/lib/format'
import { explicitPart, inferParts, withoutPart } from '@/lib/media-parts'
import { isSubtitle, subtitleName } from '@/lib/subtitles'
import type { Episode, Mapping, SourceFile, Work } from '@/types'

// Match the same-directory, same-stem or dot-suffix rule used by copy.rs.
// Keep orphan subtitles and files whose media parent is being handled as an extra.
export function standaloneSources(
  files: SourceFile[],
  mappings: Record<string, Mapping> = {},
): SourceFile[] {
  const stem = (name: string) => name.replace(/\.[^.]+$/, '').toLowerCase()
  const directory = (parent: string) => {
    const path = parent.replace(/\\/g, '/').replace(/\/$/, '')
    return /^(?:[a-z]:|\/\/)/i.test(path) ? path.toLowerCase() : path
  }
  const media = new Map<string, Set<string>>()
  for (const file of files) {
    if (!file.media || file.extra_relative_path || mappings[file.path]?.extra)
      continue
    const key = directory(file.parent)
    if (!media.has(key)) media.set(key, new Set())
    media.get(key)!.add(stem(file.name))
  }
  return files.filter((file) => {
    if (file.extra_relative_path) return true
    if (!isSubtitle(file.name)) return true
    const siblings = media.get(directory(file.parent))
    if (!siblings) return true
    let base = stem(file.name)
    while (base) {
      if (siblings.has(base)) return false
      const dot = base.lastIndexOf('.')
      if (dot < 0) break
      base = base.slice(0, dot)
    }
    return true
  })
}

export function directoryExtraMappings(
  files: SourceFile[],
  workId: string,
): Mapping[] {
  return files
    .filter((file) => file.extra_relative_path)
    .map((file) => ({
      source: file.path,
      work_id: workId,
      episode_ids: [],
      extra: true,
    }))
}

export function numbers(name: string): number[] {
  // Prefer season/episode markers and standalone bracketed numbers over title
  // digits. Keep other fields available for manual selection in their original order.
  const stem = isSubtitle(name)
    ? subtitleName(name).stem
    : name.replace(/\.[a-z\d]+$/i, '')
  return [
    ...withoutPart(stem)
      .normalize('NFKC')
      .matchAll(
        /(s\d{1,3}[ ._-]*)?(?:episode|ep|e)[ ._-]*(\d{1,3})(?![\p{L}\p{N}])|\[\s*(\d{1,3})\s*\]|【\s*(\d{1,3})\s*】|(?<![\p{L}\p{N}])(\d{1,3})(?![\p{L}\p{N}])/giu,
      ),
  ]
    .map((m) => ({
      number: Number(m[2] ?? m[3] ?? m[4] ?? m[5]),
      priority: m[1] ? 0 : m[3] || m[4] ? 1 : m[2] ? 2 : 3,
    }))
    .filter(({ number }) => number > 0 && number < 1000)
    .sort((a, b) => a.priority - b.priority)
    .map(({ number }) => number)
}

export function batchMatch(
  files: SourceFile[],
  work: Work,
  season: number,
  binding: string,
  mode: string,
  token: number,
  start: number,
  offset: number,
): Mapping[] {
  const extras = directoryExtraMappings(files, work.id)
  const parts = inferParts(files, work.kind)
  if (work.kind === 'movie') {
    return [
      ...extras,
      ...files
        .filter(
          (file) =>
            (file.media || isSubtitle(file.name)) && !file.extra_relative_path,
        )
        .map((file) => ({
          source: file.path,
          work_id: work.id,
          episode_ids: [work.episodes[0].id],
          extra: false,
          ...(parts.has(file.path) ? { part: parts.get(file.path)! } : {}),
        })),
    ]
  }
  const candidates = work.episodes.filter(
    (e) =>
      e.enabled &&
      e.season === season &&
      (!binding || e.binding_id === binding),
  )
  const sequence = new Map<string, number>()
  const sequenceParts = new Map<string, number>()
  const episodes = [...files]
    .filter(
      (f) =>
        !f.extra_relative_path &&
        (work.kind === 'music'
          ? f.media &&
            /\.(flac|mp3|m4a|aac|wav|ogg|opus|ape|alac|aiff|dsf)$/i.test(f.name)
          : (f.media || isSubtitle(f.name)) &&
            (f.hint === 'episode' || (season === 0 && f.hint === 'special'))),
    )
    .sort((a, b) => natural(a.name, b.name))
    .flatMap((file) => {
      // Each subtitle language/format in a directory follows its own sequence.
      const group = isSubtitle(file.name)
        ? `${file.parent.toLowerCase()}:${subtitleName(file.name).suffix}`
        : 'media'
      const part = parts.get(file.path) ?? 0
      const partStem = isSubtitle(file.name)
        ? subtitleName(file.name).stem
        : file.name.replace(/\.[^.]+$/, '')
      const partGroup = `${group}:${file.parent}:${withoutPart(partStem).trim().toLowerCase()}`
      const previous = part ? sequenceParts.get(partGroup) : undefined
      const i = previous ?? sequence.get(group) ?? 0
      if (previous === undefined) sequence.set(group, i + 1)
      if (part) sequenceParts.set(partGroup, i)
      const number =
        mode === 'sequence'
          ? start + i
          : (numbers(file.name)[token] ?? NaN) + offset
      let eps: Episode[]
      if (mode === 'name') {
        const matches = candidates.map((ep) => ({
          ep,
          score: nameMatchScore(file.name, ep, work),
        }))
        // A complete bilingual title takes priority over a main-title fallback.
        // Equal-strength matches remain ambiguous; never resolve them by file numbering.
        const best = Math.max(1, ...matches.map((m) => m.score))
        eps = matches.filter((m) => m.score === best).map((m) => m.ep)
      } else {
        eps = candidates.filter((e) => e.number === number)
      }
      return eps.length === 1
        ? [
            {
              source: file.path,
              work_id: work.id,
              episode_ids: [eps[0].id],
              extra: false,
              ...(part ? { part } : {}),
            },
          ]
        : []
    })
  return [...extras, ...episodes]
}

const normalizedName = (s: string) =>
  s
    .normalize('NFKC')
    .toLocaleLowerCase()
    .replace(/[\p{P}\p{S}\s_]+/gu, '')

export function nameMatches(
  filename: string,
  ep: Episode,
  work: Work,
): boolean {
  return nameMatchScore(filename, ep, work) > 0
}

// Parenthetical descriptions are optional only after a meaningful main title.
// Keep part/chapter numbers: they distinguish separate episodes, not descriptions.
function mainEpisodeTitle(title: string): string {
  const match = title
    .normalize('NFKC')
    .trim()
    .match(/^([^()]+?)\s*\(([^()]+)\)$/u)
  if (!match) return ''
  const qualifier = match[2].trim()
  if (
    /^(?:(?:part|pt|episode|ep|chapter|volume|vol|disc|cd)\b|第|(?:上|中|下|前|後|后|完結|完结)(?:[編编篇部集章卷]|$))/iu.test(
      qualifier,
    ) ||
    /^(?:[\d一二三四五六七八九十百]+|[ivxlcdm]+)$/iu.test(qualifier)
  )
    return ''
  const main = match[1].trim()
  return normalizedName(main).length >= 4 ? main : ''
}

function nameMatchScore(filename: string, ep: Episode, work: Work): 0 | 1 | 2 {
  // Do not mistake the final dotted title word or year for a file extension.
  let name = subtitleName(filename)
    .stem.normalize('NFKC')
    .replace(
      /\.(?:mkv|mp4|avi|mov|m4v|ts|m2ts|webm|wmv|mpg|mpeg|flac|mp3|m4a|aac|wav|ogg|opus|ape|alac|aiff|dsf)$/i,
      '',
    )
  // Remove the series title first: a short episode title must not match the series name.
  for (const title of [
    work.title,
    work.info?.localized_title,
    work.info?.original_title,
  ]
    .filter(Boolean)
    .sort((a, b) => b!.length - a!.length)) {
    const titleWords = title!.normalize('NFKC').match(/[\p{L}\p{N}]+/gu)
    if (titleWords?.length)
      name = name.replace(
        new RegExp(titleWords.join('[\\p{P}\\p{S}\\s_]*'), 'giu'),
        ' ',
      )
  }
  const full = normalizedName(name)
  const segments = name
    .split(/[[\]【】()（）]/)
    .map(normalizedName)
    .filter(Boolean)
  const matchesTitle = (title: string): boolean => {
    const candidate = normalizedName(title)
    if (/^\d+$/.test(candidate) || candidate.length < 2) return false
    if (/^(?:第?\d+[話话集]?|episode\d+)$/i.test(candidate)) return false
    if (/^[\p{Script=Latin}\p{N}\p{P}\p{S}\s]+$/u.test(title)) {
      const words = title.normalize('NFKC').match(/[\p{L}\p{N}]+/gu) || []
      const pattern = words.join('[\\p{P}\\p{S}\\s_]*')
      return (
        pattern.length > 0 &&
        new RegExp(
          `(?<![\\p{L}\\p{N}])${pattern}(?![\\p{L}\\p{N}])`,
          'iu',
        ).test(name)
      )
    }
    return candidate.length >= 4
      ? full.includes(candidate)
      : segments.includes(candidate) || full === candidate
  }
  const titles = [
    ep.title,
    ep.info?.localized_title,
    ep.info?.original_title,
  ].filter((title): title is string => Boolean(title))
  if (titles.some(matchesTitle)) return 2
  const part = explicitPart(filename)
  if (
    part &&
    titles.some((title) => {
      const sections = title.split(/\s+\/\s+|／/u)
      return (
        sections.length > 1 &&
        !!sections[part - 1] &&
        matchesTitle(sections[part - 1])
      )
    })
  )
    return 2
  return titles
    .map(mainEpisodeTitle)
    .some((title) => title && matchesTitle(title))
    ? 1
    : 0
}

export function extendRange(
  work: Work,
  firstId: string,
  endId: string,
): string[] {
  const first = work.episodes.find((e) => e.id === firstId),
    last = work.episodes.find((e) => e.id === endId)
  if (!first) return []
  if (!last) return [first.id]
  if (first.season !== last.season || last.number < first.number)
    return [first.id]
  const eps = work.episodes
    .filter(
      (e) =>
        e.enabled &&
        e.binding_id === first.binding_id &&
        e.season === first.season &&
        e.number >= first.number &&
        e.number <= last.number,
    )
    .sort((a, b) => a.number - b.number)
  return eps.length === last.number - first.number + 1
    ? eps.map((e) => e.id)
    : [first.id]
}
