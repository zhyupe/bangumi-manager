import { SelectField, SelectOption } from '@/components/select-field'
import { Button } from '@/components/ui/button'

export function ActionSplitButton<T extends string>({
  value,
  labels,
  onChange,
  onAction,
  disabled,
  switchDisabled,
  label,
}: {
  value: T
  labels: Record<T, string>
  onChange: (value: T) => void
  onAction: () => void
  disabled?: boolean
  switchDisabled?: boolean
  label: string
}) {
  return (
    <div className="create-split-button">
      <Button disabled={disabled} onClick={onAction}>
        {labels[value]}
      </Button>
      <SelectField
        aria-label={label}
        className="create-mode-switch"
        value={value}
        disabled={switchDisabled}
        selectedContent={<span className="sr-only">{labels[value]}</span>}
        onValueChange={(next) => onChange(next as T)}
      >
        {(Object.keys(labels) as T[]).map((key) => (
          <SelectOption key={key} value={key}>
            {labels[key]}
          </SelectOption>
        ))}
      </SelectField>
    </div>
  )
}
