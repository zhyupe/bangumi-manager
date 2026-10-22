import { CheckIcon, ChevronDownIcon } from '@radix-ui/react-icons'
import { type ReactNode, useId, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import { t } from '@/i18n'
import { cn } from '@/lib/utils'

export interface ComboboxOption {
  value: string
  label: ReactNode
  textValue: string
}

const searchable = (text: string) =>
  text
    .normalize('NFKC')
    .toLocaleLowerCase()
    .replace(/[\p{P}\p{S}\s_]+/gu, '')

export function ComboboxField({
  value,
  onValueChange,
  options,
  label,
  searchLabel,
  placeholder,
  emptyMessage = t('common.select.empty'),
  selectedContent,
  disabled,
  className,
  contentClassName,
}: {
  value: string
  onValueChange: (value: string) => void
  options: ComboboxOption[]
  label: string
  searchLabel: string
  placeholder: string
  emptyMessage?: string
  selectedContent?: ReactNode
  disabled?: boolean
  className?: string
  contentClassName?: string
}) {
  const [open, setOpen] = useState(false)
  const [search, setSearch] = useState('')
  const input = useRef<HTMLInputElement>(null)
  const id = useId()
  const changeOpen = (next: boolean) => {
    setSearch('')
    setOpen(next)
  }
  return (
    <Popover open={open && !disabled} onOpenChange={changeOpen}>
      <PopoverTrigger asChild>
        <Button
          id={id}
          variant="outline"
          role="combobox"
          aria-label={label}
          aria-expanded={open && !disabled}
          aria-haspopup="dialog"
          data-slot="combobox-trigger"
          data-value={value}
          className={cn('app-combobox', className)}
          disabled={disabled}
          onKeyDown={(event) => {
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault()
              changeOpen(true)
            }
          }}
        >
          <span data-slot="combobox-value">
            {selectedContent ??
              options.find((option) => option.value === value)?.label ??
              placeholder}
          </span>
          <ChevronDownIcon className="opacity-50" aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        className={cn('combobox-content', contentClassName)}
        align="start"
        collisionPadding={12}
        aria-label={t('common.select.optionsLabel', { label: label })}
        onOpenAutoFocus={(event) => {
          event.preventDefault()
          input.current?.focus()
        }}
        onEscapeKeyDown={(event) => {
          if (event.isComposing || event.keyCode === 229) event.preventDefault()
        }}
      >
        {open && (
          <Command
            label={searchLabel}
            defaultValue={`option:${value}`}
            vimBindings={false}
            filter={(_value, query, keywords) => {
              const text = searchable((keywords || []).join(' '))
              return query
                .normalize('NFKC')
                .trim()
                .split(/\s+/u)
                .every((part) => text.includes(searchable(part)))
                ? 1
                : 0
            }}
          >
            <CommandInput
              ref={input}
              placeholder={placeholder}
              value={search}
              onValueChange={setSearch}
            />
            <CommandList
              label={t('common.select.candidatesLabel', { label: label })}
            >
              <CommandEmpty>{emptyMessage}</CommandEmpty>
              <CommandGroup>
                {options.map((option) => (
                  <CommandItem
                    key={option.value}
                    value={`option:${option.value}`}
                    data-item-value={option.value}
                    keywords={[option.textValue]}
                    onSelect={() => {
                      if (option.value !== value) onValueChange(option.value)
                      changeOpen(false)
                    }}
                  >
                    <span className="combobox-option-content">
                      {option.label}
                    </span>
                    <CheckIcon
                      className={cn(
                        'ml-auto',
                        value !== option.value && 'invisible',
                      )}
                      aria-hidden="true"
                    />
                  </CommandItem>
                ))}
              </CommandGroup>
            </CommandList>
          </Command>
        )}
      </PopoverContent>
    </Popover>
  )
}
