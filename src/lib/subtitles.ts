export const isSubtitle = (name: string) =>
  /\.(srt|ass|ssa|sub|idx|vtt)$/i.test(name)

export const defaultSubtitlePattern =
  '([a-z]{2,3}(?:-[a-z0-9_]+)*(?:&[a-z]{2,3}(?:-[a-z0-9_]+)*)*)(?:_[^.]+)?'

let configuredPattern = ''
export function configureSubtitlePattern(pattern = '') {
  configuredPattern = pattern
}

export function subtitleName(name: string, pattern = configuredPattern) {
  if (!isSubtitle(name)) return { stem: name, suffix: '' }
  const stem = name.replace(/\.[^.]+$/, '')
  const dot = stem.lastIndexOf('.')
  const segment = dot < 0 ? '' : stem.slice(dot + 1)
  let language = ''
  try {
    const match = new RegExp(
      `^(?:${pattern.trim() || defaultSubtitlePattern})$`,
      'i',
    ).exec(segment)
    if (
      match?.[0] === segment &&
      /^[a-z0-9][a-z0-9&_-]*$/i.test(match[1] || '')
    )
      language = match[1].toLowerCase()
  } catch {
    // Invalid persisted rules must not interrupt the file list.
  }
  return {
    stem: language ? stem.slice(0, dot) : stem,
    suffix:
      `${language ? `.${language}` : ''}.${name.split('.').at(-1)}`.toLowerCase(),
  }
}
