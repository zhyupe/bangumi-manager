import { isSubtitle, subtitleName } from '@/lib/subtitles'
import type { SourceFile } from '@/types'

const marker =
  /(?:^|[\s._~()[\]-])(?:part|pt|cd|disc|disk|dvd)[ ._-]*0?([1-8])(?=$|[\s._~()[\]-])/i
const stem = (name: string) =>
  isSubtitle(name) ? subtitleName(name).stem : name.replace(/\.[^.]+$/, '')

export function explicitPart(name: string): number {
  return Number(
    [...stem(name).matchAll(new RegExp(marker.source, 'gi'))].at(-1)?.[1] ?? 0,
  )
}

export function withoutPart(name: string): string {
  return name.replace(new RegExp(marker.source, 'gi'), ' ')
}

// Bare movie numbers are only parts when their siblings form a complete 1..N set.
// Episode numbers and isolated numeric suffixes remain ordinary matching inputs.
export function inferParts(
  files: Pick<SourceFile, 'path' | 'name' | 'parent'>[],
  kind: string,
): Map<string, number> {
  const result = new Map<string, number>()
  if (kind === 'music') return result
  const groups = new Map<string, { path: string; part: number }[]>()
  for (const file of files) {
    const part = explicitPart(file.name)
    if (part) result.set(file.path, part)
    if (part || kind !== 'movie') continue
    const match = stem(file.name).match(/^(.+?)\s+-\s+(0?[1-8])$/)
    if (!match) continue
    const suffix = isSubtitle(file.name)
      ? subtitleName(file.name).suffix
      : 'media'
    const key = `${file.parent.replace(/\\/g, '/').toLowerCase()}:${match[1].toLowerCase()}:${suffix}`
    const group = groups.get(key) ?? []
    group.push({ path: file.path, part: Number(match[2]) })
    groups.set(key, group)
  }
  for (const group of groups.values()) {
    const numbers = [...new Set(group.map((f) => f.part))].sort((a, b) => a - b)
    if (numbers.length < 2 || numbers.some((n, i) => n !== i + 1)) continue
    for (const file of group) result.set(file.path, file.part)
  }
  return result
}

export const partSlots = (part = 0) =>
  part ? [part] : Array.from({ length: 9 }, (_, i) => i)
