import { Badge } from '@/components/ui/badge'
import { t } from '@/i18n'
import type { catalogCollection } from './catalog'

interface Props {
  collection: ReturnType<typeof catalogCollection>
  scan: 'current' | 'offline' | 'unscanned'
}

export function SeasonCollectionSummary({ collection, scan }: Props) {
  const { seasons, complete } = collection
  const known = seasons.some((season) => season.total > 0)
  const current = scan === 'current'
  return (
    <Badge
      className="work-collection-summary"
      tone={
        scan === 'offline'
          ? 'amber'
          : current && known && complete === seasons.length
            ? 'green'
            : 'gray'
      }
    >
      {scan === 'unscanned'
        ? t('library.coverage.awaitingScan')
        : scan === 'offline'
          ? t('library.coverage.offline')
          : known
            ? t('library.coverage.seasons', {
                completed: complete,
                total: seasons.length,
              })
            : t('library.coverage.noEpisodes')}
    </Badge>
  )
}
