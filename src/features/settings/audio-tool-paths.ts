export function audioToolPaths(directory: string) {
  if (!directory.trim()) return { ffmpeg: '', ffprobe: 'ffprobe' }
  const separator = directory.includes('\\') ? '\\' : '/'
  const root = directory.replace(/[\\/]+$/, '')
  const extension = /^(?:[a-z]:|\\\\)/i.test(directory) ? '.exe' : ''
  return {
    ffmpeg: `${root}${separator}ffmpeg${extension}`,
    ffprobe: `${root}${separator}ffprobe${extension}`,
  }
}

export function audioToolDirectory(
  ffmpeg: string,
  ffprobe: string,
): string | null {
  if ((!ffmpeg || ffmpeg === 'ffmpeg') && ffprobe === 'ffprobe') return ''
  const match = ffprobe.match(/^(.*[\\/])ffprobe(?:\.exe)?$/i)
  if (!match) return null
  const directory = match[1]
  const expected = audioToolPaths(directory).ffmpeg
  const normalize = (path: string) =>
    /^[a-z]:|^\\\\/i.test(path) ? path.replace(/\\/g, '/').toLowerCase() : path
  if (ffmpeg && normalize(ffmpeg) !== normalize(expected)) return null
  return directory.replace(/(?<!:)[\\/]$/, '') || '/'
}
