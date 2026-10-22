import type { RemoteSubject } from '@/types'
import type { CueAnalysis, TrackChoice } from './types'

export function cueChoices(
  analysis: CueAnalysis,
  subject?: RemoteSubject,
  disc = 1,
): TrackChoice[] {
  const remote = (subject?.episodes ?? [])
    .filter((e) => e.season === disc && e.sort > 0)
    .sort((a, b) => a.sort - b.sort)
  const automatic =
    remote.length === analysis.tracks.length &&
    remote.every((e, i) => e.sort === i + 1)
  return analysis.tracks.map((track, i) => ({
    number: track.number,
    remote_id: automatic ? remote[i].id : '',
    title: automatic ? remote[i].title || track.title : track.title,
    performer: automatic
      ? remote[i].info?.artist || track.performer
      : track.performer,
  }))
}
