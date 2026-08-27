import { useTranslation } from '../../lib/i18n'
import { useEscapeKey } from '../../lib/useEscapeKey'
import type { EditScope } from '../../states/calendar'

/// Asks which occurrences an action reaches, for an event that repeats.
///
/// Only asked when it can matter — an event that repeats, and an action that
/// could go either way. Answering for the reader would be guessing at the one
/// thing they alone know, and guessing wrong here changes appointments they
/// did not open.
export function ScopeAsk({
  action,
  onChoose,
  onCancel,
}: {
  action: 'save' | 'delete'
  onChoose: (scope: EditScope) => void
  onCancel: () => void
}) {
  const { t } = useTranslation()
  useEscapeKey(onCancel)

  return (
    <div
      className="fixed inset-0 z-[70] flex items-center justify-center bg-black/40 p-4"
      onMouseDown={(mouse) => {
        if (mouse.target === mouse.currentTarget) onCancel()
      }}
    >
      <div className="w-full max-w-sm rounded-2xl border border-border bg-app p-5 shadow-xl">
        <h2 className="text-sm font-semibold text-primary">
          {action === 'delete'
            ? t('calendar.scopeDeleteTitle', { defaultValue: 'Delete a repeating event' })
            : t('calendar.scopeSaveTitle', { defaultValue: 'Change a repeating event' })}
        </h2>
        <p className="mt-1.5 text-[0.6875rem] text-secondary">
          {action === 'delete'
            ? t('calendar.scopeDeleteHint', {
                defaultValue: 'This event is one of a series. What should be deleted?',
              })
            : t('calendar.scopeSaveHint', {
                defaultValue: 'This event is one of a series. What should the change apply to?',
              })}
        </p>

        <div className="mt-4 flex flex-col gap-2">
          <button
            type="button"
            onClick={() => onChoose('occurrence')}
            className="rounded-xl border border-border bg-raised px-3 py-2.5 text-left text-xs font-medium text-primary transition-colors hover:bg-hover cursor-pointer"
          >
            {t('calendar.scopeThisOne', { defaultValue: 'This event only' })}
          </button>
          <button
            type="button"
            onClick={() => onChoose('series')}
            className="rounded-xl border border-border bg-raised px-3 py-2.5 text-left text-xs font-medium text-primary transition-colors hover:bg-hover cursor-pointer"
          >
            {t('calendar.scopeAll', { defaultValue: 'The whole series' })}
          </button>
        </div>

        <div className="mt-4 flex justify-end">
          <button
            type="button"
            onClick={onCancel}
            className="rounded-xl px-3 py-2 text-xs font-medium text-secondary transition-colors hover:bg-hover hover:text-primary cursor-pointer"
          >
            {t('calendar.cancel', { defaultValue: 'Cancel' })}
          </button>
        </div>
      </div>
    </div>
  )
}
