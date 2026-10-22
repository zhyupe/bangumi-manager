import { type ComponentProps, type ReactNode, useId } from 'react'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { cn } from '@/lib/utils'

type Props = Omit<
  ComponentProps<typeof SelectTrigger>,
  'value' | 'onChange' | 'children'
> & {
  value: string | number | undefined
  onValueChange: (value: string) => void
  children: ReactNode
  contentClassName?: string
  selectedContent?: ReactNode
}
// Prefixing every value lets forms keep their empty/reset option without reserving an application ID.
export function SelectField({
  value,
  onValueChange,
  children,
  className,
  contentClassName,
  selectedContent,
  id,
  disabled,
  ...props
}: Props) {
  const generatedId = useId()
  return (
    <Select
      value={`value:${value ?? ''}`}
      onValueChange={(v) => onValueChange(v.slice(6))}
      disabled={disabled}
    >
      <SelectTrigger
        data-value={String(value ?? '')}
        id={id || generatedId}
        className={cn('app-select', className)}
        {...props}
      >
        <SelectValue>{selectedContent}</SelectValue>
      </SelectTrigger>
      <SelectContent
        position="popper"
        className={cn('app-select-content', contentClassName)}
      >
        {children}
      </SelectContent>
    </Select>
  )
}
export function SelectOption({
  value,
  children,
  ...props
}: Omit<ComponentProps<typeof SelectItem>, 'value'> & {
  value: string | number
}) {
  return (
    <SelectItem data-value={String(value)} value={`value:${value}`} {...props}>
      {children}
    </SelectItem>
  )
}
