import type { LibraryIndex, Work } from './library'
import type { SourceFile } from './media'
import type { Settings } from './settings'
import type { Plan, QueueState } from './tasks'

export interface Snapshot {
  queue?: QueueState
  settings: Settings
  works: Work[]
  sources: SourceFile[]
  index: LibraryIndex
  plans: Plan[]
  data_dir: string
}
