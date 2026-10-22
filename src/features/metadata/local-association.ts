import type { RemoteSubject, Work } from '@/types'

type Identity = Pick<RemoteSubject, 'provider' | 'kind' | 'id'>

export const subjectBinding = (work: Work, subject: Identity) =>
  work.bindings.find(
    (binding) =>
      binding.provider === subject.provider &&
      binding.source_kind === subject.kind &&
      binding.remote_id === subject.id,
  )

export function linkedWorks(works: Work[], subject: Identity): Work[] {
  if (!subject.id) return []
  const bound = works.filter((work) => subjectBinding(work, subject))
  if (bound.length) return bound
  // NFO imports can have database IDs before a remote binding is created.
  return works.filter(
    (work) =>
      work.kind === subject.kind &&
      ((subject.provider === 'tmdb' && work.tmdb_id === subject.id) ||
        work.info?.external_ids?.[subject.provider] === subject.id),
  )
}

export function importAssociation(
  works: Work[],
  subject: Identity,
  preferredId = '',
) {
  const matches = linkedWorks(works, subject)
  const work =
    matches.length === 1
      ? matches[0]
      : matches.length > 1
        ? matches.find((work) => work.id === preferredId)
        : works.find(
            (work) => work.id === preferredId && work.kind === subject.kind,
          )
  const binding = work && subjectBinding(work, subject)
  const seasons = work
    ? [
        ...work.episodes.map((e) => e.season),
        ...work.bindings.map((b) => b.season),
      ].filter((season) => season > 0)
    : []
  const season =
    binding?.season ??
    (matches.length
      ? seasons.length
        ? Math.min(...seasons)
        : 1
      : Math.max(0, ...seasons) + 1)
  return { work, binding, season }
}
