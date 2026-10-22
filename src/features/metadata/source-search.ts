import type { SourceFile } from '@/types'

export const preferredProvider = (kind: string) =>
  kind === 'music' ? 'bangumi' : 'tmdb'
const audio = /\.(?:flac|mp3|m4a|aac|wav|ogg|opus|ape|alac|aiff|dsf)$/i
const video = /\.(?:mkv|mp4|avi|mov|m4v|ts|m2ts|webm|wmv|mpg|mpeg)$/i
const technical =
  /^(?:\d{3,4}[pi]|\d{3,4}x\d{3,4}|[hx]26[45]|hevc|avc|av1|flac|aac|alac|dts|ac3|eac3|truehd|pcm|bd(?:rip|remux)?|blu-?ray|web(?:rip|dl)?|remux|mkv|mp4|10bit|8bit|hi10p|hdr10?|dv|sdr|uhd|2160|1080|720|chs|cht|jpn|eng|zh|cn|[a-f\d]{8}|(?:19|20)\d{2}|0\d+)$/i
const genericFolder =
  /^(?:incoming|downloads?|torrents?|temp|tmp|movies?|tv|shows?|music|videos?|media|audio|anime|completed|nas|share|整理|待整理|下载|电影|剧集|电视剧|音乐|动画|视频|音频)$/i

function words(value: string, filename = false): string[] {
  const clean = (filename ? value.replace(/\.[^.]+$/, '') : value)
    .normalize('NFKC')
    .replace(/_/g, ' ')
    .replace(/\bS\d{1,3}E\d{1,4}(?:-?E?\d{1,4})?\b/gi, ' ')
    .replace(/\b(?:season|disc|disk|cd|track|episode|ep)\s*\d{1,4}\b/gi, ' ')
    .replace(/第[\d一二三四五六七八九十百]+[季集话話碟盘]/g, ' ')
    .replace(/[[（(【]\s*\d{1,4}\s*[\]）)】]/g, ' ')
    .replace(/\b[hx][ ._-]?26[45]\b|\bweb[ ._-]?dl\b|\bblu[ ._-]?ray\b/gi, ' ')
    .replace(/\b(?:ddp?|aac|dts(?:-hd)?|truehd)[ ._-]?\d\.\d(?:ch)?\b/gi, ' ')
    .replace(/^\s*\d{1,3}[\s._-]+|\s+-\s+\d{1,3}(?=\s|$)/g, ' ')
  return clean
    .split(/[[\]【】()（）\s_.+-]+/u)
    .filter(
      (word) =>
        word &&
        !technical.test(word) &&
        !/^[\d:]+$/.test(word) &&
        (word.length > 1 || /\p{L}/u.test(word)),
    )
}

function folderWords(file: SourceFile): string[] {
  const parent = file.parent || file.path.replace(/[\\/][^\\/]*$/, '')
  const parts = parent.split(/[\\/]+/).filter(Boolean)
  // Drive letters and UNC server/share names are not title hints.
  const directories = /^[\\/]{2}/.test(parent)
    ? parts.slice(2)
    : parts.filter((p) => !/^[a-z]:$/i.test(p))
  for (const folder of directories.slice(-3).reverse()) {
    const useful = words(folder).filter((word) => !genericFolder.test(word))
    if (useful.length) return useful
  }
  return []
}

function selectedMedia(files: SourceFile[]): SourceFile[] {
  const media = files.filter((f) => f.media)
  return media.length ? media : files
}

function shared(lists: string[][]): { text: string; count: number }[] {
  const counts = new Map<string, { text: string; count: number }>()
  for (const list of lists) {
    const seen = new Set<string>()
    for (const text of list) {
      const key = text.toLocaleLowerCase()
      if (seen.has(key)) continue
      seen.add(key)
      const previous = counts.get(key) || { text, count: 0 }
      previous.count++
      counts.set(key, previous)
    }
  }
  return [...counts.values()]
    .filter((v) => v.count >= Math.max(1, Math.ceil(lists.length * 0.6)))
    .sort((a, b) => b.count - a.count)
    .slice(0, 24)
}

export function commonKeywords(
  files: SourceFile[],
): { text: string; count: number }[] {
  const selected = selectedMedia(files)
  if (!selected.length) return []
  const names = shared(selected.map((f) => words(f.name, true)))
  const folders = shared(selected.map(folderWords))
  const hasCommonFolder = folders.some((v) => v.count === selected.length)
  if (selected.every((f) => audio.test(f.name)) && hasCommonFolder)
    return folders
  if (names.some((v) => v.count === selected.length)) return names
  return hasCommonFolder ? folders : names
}

export function suggestSourceImport(files: SourceFile[], fallbackKind = 'tv') {
  const selected = selectedMedia(files)
  const hasVideo = selected.some((f) => video.test(f.name))
  const kind =
    selected.length && selected.every((f) => audio.test(f.name))
      ? 'music'
      : hasVideo
        ? selected.some((f) =>
            /\bS\d+E\d+\b/i.test(f.name.replace(/_/g, ' ')),
          ) || fallbackKind !== 'movie'
          ? 'tv'
          : 'movie'
        : fallbackKind
  return {
    kind,
    keyword: commonKeywords(selected)
      .filter((v) => v.count === selected.length)
      .map((v) => v.text)
      .join(' '),
  }
}
