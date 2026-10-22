import { t } from '@/i18n'
import type { Work } from '@/types'
import { ComboboxField } from './combobox-field'

export function WorkSelect({
  works,
  value,
  onValueChange,
  disabled,
}: {
  works: Work[]
  value: string
  onValueChange: (value: string) => void
  disabled?: boolean
}) {
  return (
    <ComboboxField
      label={t('organize.mapping.work')}
      searchLabel={t('library.search.label')}
      placeholder={t('library.search.placeholder')}
      emptyMessage={t('library.empty.noResults')}
      value={value}
      onValueChange={onValueChange}
      disabled={disabled}
      selectedContent={
        works.find((work) => work.id === value)?.title || t('common.workSelect.placeholder')
      }
      contentClassName="work-combobox-content"
      options={[
        { value: '', label: t('common.workSelect.clear'), textValue: t('common.workSelect.clear') },
        ...works.map((work) => ({
          value: work.id,
          textValue: [
            work.title,
            work.info?.localized_title,
            work.info?.original_title,
            work.year,
            ...work.bindings.map((binding) => binding.source_title),
          ]
            .filter(Boolean)
            .join(' '),
          label: (
            <span className="work-option">
              <span>{work.title}</span>
              {[
                ...new Set([
                  work.info?.localized_title,
                  work.info?.original_title,
                ]),
              ]
                .filter((title) => title && title !== work.title)
                .map((title) => (
                  <span key={title} className="work-option-detail">
                    {title}
                  </span>
                ))}
              <span className="work-option-detail">
                {[
                  work.year,
                  { tv: t('common.media.tv'), movie: t('common.media.movie'), music: t('common.media.music') }[
                    work.kind
                  ],
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </span>
          ),
        })),
      ]}
    />
  )
}
