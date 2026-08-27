import { useState } from 'react'
import { useValue } from '@legendapp/state/react'
import { Trash2, X } from 'lucide-react'
import { useTranslation } from '../../lib/i18n'
import { calendar$, closeEditor, deleteEvent, saveEvent, type EventDraft } from '../../states/calendar'

/// Creates or edits one event.
///
/// Saving never notifies anyone — the backend declares that explicitly on
/// every write — so this offers no "send invitations" control it could not
/// honour. Attendees are shown when an event has them but not edited here:
/// changing who is invited is what actually mails people, and belongs with
/// meeting invitations rather than with editing a time.
export function EventEditor() {
  const { t } = useTranslation()
  const draft = useValue(calendar$.editing)
  const saving = useValue(calendar$.saving)
  const error = useValue(calendar$.error)
  const calendars = useValue(calendar$.calendars)
  const [local, setLocal] = useState<EventDraft | null>(null)

  // Adopt the draft once per opening, so typing is not overwritten by the
  // observable it came from.
  const event = local && draft && local.id === draft.id ? local : draft
  if (!draft || !event) return null

  const set = (patch: Partial<EventDraft>) => setLocal({ ...event, ...patch })
  const isNew = !event.id
  const invalid = !event.subject.trim() || event.end < event.start

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4" onClick={closeEditor}>
      <div
        className="w-full max-w-md rounded-2xl border border-border bg-app p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-sm font-semibold text-primary">
            {isNew
              ? t('calendar.newEvent', { defaultValue: 'New event' })
              : t('calendar.editEvent', { defaultValue: 'Edit event' })}
          </h2>
          <button
            type="button"
            onClick={closeEditor}
            className="flex h-7 w-7 items-center justify-center rounded-lg text-secondary hover:bg-hover hover:text-primary cursor-pointer"
            aria-label={t('calendar.close', { defaultValue: 'Close' })}
          >
            <X size={15} />
          </button>
        </div>

        <div className="flex flex-col gap-3">
          <Labelled label={t('calendar.subject', { defaultValue: 'Title' })}>
            <input value={event.subject} onChange={(e) => set({ subject: e.target.value })} autoFocus className={inputClass} />
          </Labelled>

          <Labelled label={t('calendar.location', { defaultValue: 'Location' })}>
            <input value={event.location ?? ''} onChange={(e) => set({ location: e.target.value })} className={inputClass} />
          </Labelled>

          <Labelled label={t('calendar.reminder', { defaultValue: 'Reminder' })}>
            <select
              value={event.reminder_minutes ?? ''}
              onChange={(e) =>
                set({ reminder_minutes: e.target.value === '' ? null : Number(e.target.value) })
              }
              className={inputClass}
            >
              <option value="">{t('calendar.reminderNone', { defaultValue: 'None' })}</option>
              {REMINDER_CHOICES.map((minutes) => (
                <option key={minutes} value={minutes}>
                  {reminderLabel(minutes, t)}
                </option>
              ))}
            </select>
          </Labelled>

          <Labelled label={t('calendar.description', { defaultValue: 'Notes' })}>
            <textarea
              value={event.description ?? ''}
              onChange={(e) => set({ description: e.target.value })}
              rows={4}
              className={`${inputClass} resize-y`}
            />
          </Labelled>

          <div className="flex gap-3">
            <Labelled label={t('calendar.starts', { defaultValue: 'Starts' })}>
              <DateAndTime
                value={event.start}
                allDay={event.all_day}
                onChange={(start) =>
                  // Keep the duration when the start moves, which is what
                  // moving a meeting to another time means.
                  set({ start, end: start + (event.end - event.start) })
                }
              />
            </Labelled>
            <Labelled label={t('calendar.ends', { defaultValue: 'Ends' })}>
              <DateAndTime
                value={event.end}
                allDay={event.all_day}
                onChange={(end) => set({ end })}
              />
            </Labelled>
          </div>

          <label className="flex items-center gap-2 text-xs text-primary">
            <input type="checkbox" checked={event.all_day} onChange={(e) => set({ all_day: e.target.checked })} />
            {t('calendar.allDay', { defaultValue: 'All day' })}
          </label>

          {isNew && calendars.filter((calendar) => calendar.enabled).length > 1 && (
            <Labelled label={t('calendar.calendar', { defaultValue: 'Calendar' })}>
              <select
                value={`${event.accountId} ${event.calendar_id}`}
                onChange={(e) => {
                  const [accountId, calendar_id] = e.target.value.split(' ')
                  set({ accountId, calendar_id })
                }}
                className={inputClass}
              >
                {calendars
                  .filter((calendar) => calendar.enabled)
                  .map((calendar) => (
                    <option key={`${calendar.accountId}:${calendar.id}`} value={`${calendar.accountId} ${calendar.id}`}>
                      {calendar.name}
                    </option>
                  ))}
              </select>
            </Labelled>
          )}

          {event.attendees.length > 0 && (
            <p className="text-[0.6875rem] text-secondary">
              {t('calendar.attendeesNote', {
                defaultValue: 'This event has {count} guests. Editing it here does not notify them.',
                count: event.attendees.length,
              })}
            </p>
          )}

          {event.end < event.start && (
            <p className="text-[0.6875rem] text-rose-500">
              {t('calendar.endsBeforeStart', { defaultValue: 'It ends before it starts.' })}
            </p>
          )}
          {error && <p className="text-[0.6875rem] text-rose-500">{error}</p>}
        </div>

        <div className="mt-5 flex items-center justify-between gap-2">
          {!isNew ? (
            <button
              type="button"
              onClick={() => void deleteEvent(event)}
              className="inline-flex items-center gap-1.5 rounded-xl px-3 py-2 text-xs font-medium text-rose-500 transition-colors hover:bg-rose-500/10 cursor-pointer"
            >
              <Trash2 size={13} />
              {t('calendar.delete', { defaultValue: 'Delete' })}
            </button>
          ) : (
            <span />
          )}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={closeEditor}
              className="rounded-xl px-3 py-2 text-xs font-medium text-secondary transition-colors hover:bg-hover hover:text-primary cursor-pointer"
            >
              {t('calendar.cancel', { defaultValue: 'Cancel' })}
            </button>
            <button
              type="button"
              disabled={invalid || saving}
              onClick={() => void saveEvent(event)}
              className="rounded-xl bg-accent px-4 py-2 text-xs font-semibold text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50 cursor-pointer"
            >
              {t('calendar.save', { defaultValue: 'Save' })}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

const inputClass =
  'w-full rounded-xl border border-border bg-raised px-3 py-2 text-xs text-primary outline-none transition-all focus:border-transparent focus:bg-chats focus:ring-1 focus:ring-accent'

/// The usual ladder of reminder times, in minutes before the start.
const REMINDER_CHOICES = [0, 5, 10, 15, 30, 60, 120, 24 * 60, 2 * 24 * 60, 7 * 24 * 60]

function reminderLabel(minutes: number, t: ReturnType<typeof useTranslation>['t']): string {
  if (minutes === 0) return t('calendar.reminderAtStart', { defaultValue: 'At the start' })
  if (minutes % (7 * 24 * 60) === 0)
    return t('calendar.reminderWeeks', {
      defaultValue: '{count} weeks before',
      count: minutes / (7 * 24 * 60),
    })
  if (minutes % (24 * 60) === 0)
    return t('calendar.reminderDays', {
      defaultValue: '{count} days before',
      count: minutes / (24 * 60),
    })
  if (minutes % 60 === 0)
    return t('calendar.reminderHours', { defaultValue: '{count} h before', count: minutes / 60 })
  return t('calendar.reminderMinutes', { defaultValue: '{count} min before', count: minutes })
}

function Labelled({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex w-full flex-col gap-1.5">
      <span className="pl-0.5 text-[0.6875rem] font-semibold text-secondary">{label}</span>
      {children}
    </label>
  )
}

/// Epoch seconds to what a datetime-local input expects, in local time — the
/// timezone the person editing is in, which is the only one they can reason
/// about.
/// A date and a time, as two controls rather than one `datetime-local`.
///
/// The engine this app runs on renders `datetime-local` with a date picker and
/// no way to reach the time, which left every event stuck at whatever hour it
/// already had. A plain date input and a list of times work the same
/// everywhere, and are what the calendars people are used to offer anyway.
function DateAndTime({
  value,
  allDay,
  onChange,
}: {
  value: number
  allDay: boolean
  onChange: (epochSeconds: number) => void
}) {
  const date = new Date(value * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  const dateValue = `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
  const timeValue = `${pad(date.getHours())}:${pad(date.getMinutes())}`

  const setDate = (text: string) => {
    const [year, month, day] = text.split('-').map(Number)
    if (!year || !month || !day) return
    const next = new Date(value * 1000)
    next.setFullYear(year, month - 1, day)
    onChange(Math.floor(next.getTime() / 1000))
  }

  const setTime = (text: string) => {
    const [hours, minutes] = text.split(':').map(Number)
    if (Number.isNaN(hours) || Number.isNaN(minutes)) return
    const next = new Date(value * 1000)
    next.setHours(hours, minutes, 0, 0)
    onChange(Math.floor(next.getTime() / 1000))
  }

  return (
    <div className="flex gap-1.5">
      <input type="date" value={dateValue} onChange={(e) => setDate(e.target.value)} className={inputClass} />
      {/* An all-day event has no hour to set, so none is offered. */}
      {!allDay && (
        <select value={timeValue} onChange={(e) => setTime(e.target.value)} className={`${inputClass} w-28`}>
          {/* The event's own time, when it does not fall on a quarter hour:
              an invitation at 14:37 must survive being looked at. */}
          {!QUARTER_HOURS.includes(timeValue) && <option value={timeValue}>{timeValue}</option>}
          {QUARTER_HOURS.map((time) => (
            <option key={time} value={time}>
              {time}
            </option>
          ))}
        </select>
      )}
    </div>
  )
}

/// Every quarter hour of the day, which is how calendars have always offered
/// times to pick from.
const QUARTER_HOURS = Array.from({ length: 24 * 4 }, (_, index) => {
  const hours = Math.floor(index / 4)
  const minutes = (index % 4) * 15
  return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}`
})

