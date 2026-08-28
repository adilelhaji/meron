import { useTranslation } from '../../lib/i18n'
import { useEscapeKey } from '../../lib/useEscapeKey'

/// Asks whether the people on a meeting should be told.
///
/// Always asked, never remembered: sending mail to other people is not a
/// preference to be inferred from what was chosen last time, and the whole
/// point of the question is that the reader knows a message is about to leave.
export function NotifyAsk({
  action,
  people,
  onChoose,
  onCancel,
}: {
  action: 'save' | 'delete'
  people: string[]
  onChoose: (notify: boolean) => void
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
            ? t('calendar.notifyCancelTitle', { defaultValue: 'Cancel this meeting?' })
            : t('calendar.notifyTitle', { defaultValue: 'Tell the people on it?' })}
        </h2>
        <p className="mt-1.5 text-[0.6875rem] text-secondary">
          {action === 'delete'
            ? t('calendar.notifyCancelHint', {
                defaultValue: 'A cancellation can be sent to everyone on the meeting.',
              })
            : t('calendar.notifyHint', {
                defaultValue: 'An invitation can be sent to everyone on the meeting.',
              })}
        </p>

        {/* Named, not counted: the reader is about to mail these people and
            should see who they are before it happens. */}
        <ul className="mt-3 max-h-32 overflow-y-auto rounded-xl bg-raised px-3 py-2">
          {people.map((person) => (
            <li key={person} className="truncate text-[0.6875rem] text-primary">
              {person}
            </li>
          ))}
        </ul>

        <div className="mt-4 flex flex-col gap-2">
          <button
            type="button"
            onClick={() => onChoose(true)}
            className="rounded-xl bg-accent px-3 py-2.5 text-xs font-semibold text-white transition-opacity hover:opacity-90 cursor-pointer"
          >
            {action === 'delete'
              ? t('calendar.notifySendCancel', { defaultValue: 'Send a cancellation' })
              : t('calendar.notifySend', { defaultValue: 'Send it' })}
          </button>
          <button
            type="button"
            onClick={() => onChoose(false)}
            className="rounded-xl border border-border bg-raised px-3 py-2.5 text-xs font-medium text-primary transition-colors hover:bg-hover cursor-pointer"
          >
            {t('calendar.notifySilent', { defaultValue: 'Save without telling anyone' })}
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
