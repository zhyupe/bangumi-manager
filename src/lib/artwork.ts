import { convertFileSrc } from '@tauri-apps/api/core'
import type { Work } from '@/types'
export function artwork(work: Work): string {
  return work.info?.local_poster
    ? convertFileSrc(work.info.local_poster)
    : work.poster
}
