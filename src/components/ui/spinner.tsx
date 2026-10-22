import { ReloadIcon as Loader2Icon } from '@radix-ui/react-icons'
import { t } from '@/i18n'
import { cn } from '@/lib/utils'

function Spinner({
  className,
  ...props
}: React.ComponentProps<typeof Loader2Icon>) {
  return (
    <Loader2Icon
      role="status"
      aria-label={t('common.status.loading')}
      className={cn('size-4 animate-spin', className)}
      {...props}
    />
  )
}

export { Spinner }
