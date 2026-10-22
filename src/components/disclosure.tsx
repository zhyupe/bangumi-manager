import { ChevronRightIcon } from '@radix-ui/react-icons'
import { Children, type ComponentProps, isValidElement } from 'react'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { cn } from '@/lib/utils'
export function Disclosure({
  children,
  className,
  ...props
}: ComponentProps<typeof Collapsible>) {
  const items = Children.toArray(children)
  return (
    <Collapsible className={cn('disclosure', className)} {...props}>
      {items.filter(
        (item) => isValidElement(item) && item.type === DisclosureTrigger,
      )}
      <CollapsibleContent>
        {items.filter(
          (item) => !isValidElement(item) || item.type !== DisclosureTrigger,
        )}
      </CollapsibleContent>
    </Collapsible>
  )
}
export function DisclosureTrigger({
  children,
  className,
  ...props
}: ComponentProps<typeof Button>) {
  return (
    <CollapsibleTrigger asChild>
      <Button
        variant="ghost"
        className={cn('disclosure-trigger', className)}
        {...props}
      >
        <ChevronRightIcon />
        {children}
      </Button>
    </CollapsibleTrigger>
  )
}
