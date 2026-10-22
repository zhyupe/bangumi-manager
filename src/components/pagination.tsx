import { Button } from '@/components/ui/button'
import { t } from '@/i18n'

export function Pages({
  page,
  count,
  onChange,
  size = 80,
}: {
  page: number
  count: number
  onChange: (p: number) => void
  size?: number
}) {
  const pages = Math.max(1, Math.ceil(count / size))
  return (
    <div className="pagination">
      <span>
        {t('common.pagination.summary', {
          count: count,
          page: Math.min(page, pages),
          pages: pages,
        })}
      </span>
      <Button
        variant="secondary"
        size="sm"
        disabled={page <= 1}
        onClick={() => onChange(page - 1)}
      >
        {t('common.pagination.previous')}
      </Button>
      <Button
        variant="secondary"
        size="sm"
        disabled={page >= pages}
        onClick={() => onChange(page + 1)}
      >
        {t('common.pagination.next')}
      </Button>
    </div>
  )
}
