import { describe, expect, it } from 'vitest'
import { audioToolDirectory, audioToolPaths } from './audio-tool-paths'

describe('shared audio tool paths', () => {
  it('handles system PATH, Windows, UNC and Unix folders', () => {
    expect(audioToolPaths('')).toEqual({ ffmpeg: '', ffprobe: 'ffprobe' })
    for (const directory of [
      'D:\\Tools',
      'D:/Tools',
      '\\\\server\\tools',
      '/opt/tools',
      '/',
      'D:\\',
    ]) {
      const paths = audioToolPaths(directory)
      expect(audioToolDirectory(paths.ffmpeg, paths.ffprobe)).not.toBeNull()
      expect(
        audioToolPaths(audioToolDirectory(paths.ffmpeg, paths.ffprobe)!),
      ).toEqual(paths)
    }
    expect(audioToolPaths('D:\\Tools').ffprobe).toBe('D:\\Tools\\ffprobe.exe')
  })
  it('preserves separately configured executables and recognizes inferred siblings', () => {
    expect(
      audioToolDirectory('D:/One/ffmpeg.exe', 'D:/Two/ffprobe.exe'),
    ).toBeNull()
    expect(audioToolDirectory('', 'D:/Tools/ffprobe.exe')).toBe('D:/Tools')
    expect(
      audioToolDirectory('D:/TOOLS/FFMPEG.EXE', 'D:/Tools/ffprobe.exe'),
    ).toBe('D:/Tools')
    expect(audioToolDirectory('', 'ffprobe')).toBe('')
  })
})
