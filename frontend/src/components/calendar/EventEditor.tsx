import { useEffect, useState } from 'react'
import { useValue } from '@legendapp/state/react'
import { Trash2, X } from 'lucide-react'
import { useTranslation } from '../../lib/i18n'
import {
  calendar$,
  closeEditor,
  deleteEvent,
  loadSeriesRule,
  saveEvent,
  type EditScope,
  type EventDraft,
  type Frequency,
  type Recurrence,
} from '../../states/calendar'
import { ScopeAsk } from './ScopeAsk'

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
  const [pendingScope, setPendingScope] = useState<'save' | 'delete' | null>(null)
  // The rule is not carried with an occurrence, so it is fetched once when a
  // repeating event is opened.
  const [ruleLoaded, setRuleLoaded] = useState('')

  // The rule behind a series, fetched once per opening. Runs before the early
  // return, since a hook behind a condition changes the count between renders.
  useEffect(() => {
    if (!draft?.id || !draft.is_recurring || !draft.series_id) return
    if (ruleLoaded === draft.id) return
    setRuleLoaded(draft.id)
    let live = true
    void loadSeriesRule(draft).then((recurrence) => {
      // Merged into whatever is on screen rather than replacing it: the reader
      // may have typed while the answer was in flight.
      if (live && recurrence) {
        setLocal((current) => ({ ...(current ?? draft), recurrence }))
      }
    })
    return () => {
      live = false
    }
  }, [draft?.id, draft?.is_recurring, draft?.series_id, ruleLoaded])

  // Adopt the draft once per opening, so typing is not overwritten by the
  // observable it came from.
  const event = local && draft && local.id === draft.id ? local : draft
  if (!draft || !event) return null

  const set = (patch: Partial<EventDraft>) => setLocal({ ...event, ...patch })
  // Which action is waiting on "this one or all of them?", if any.
  const asking = pendingScope
  const isNew = !event.id
  const invalid = !event.subject.trim() || event.end < event.start
  // Only an occurrence of a real series raises the question; a one-off, or an
  // event being created, has only one answer.
  const repeats = !isNew && event.is_recurring && Boolean(event.series_id)

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4" onClick={closeEditor}>
      <div
        className="flex max-h-[calc(100vh-2rem)] w-full max-w-md flex-col rounded-2xl border border-border bg-app shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-center justify-between px-5 pb-3 pt-5">
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

        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5 pb-1">
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

          <div className="flex flex-col gap-3">
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

          {/* The rule shows for a new event and for one whose series can be
              reached; a change to it takes effect when the save applies to the
              whole series, which is what the question on saving decides. */}
          {isNew || repeats ? (
            <RepeatFields
              rule={event.recurrence ?? null}
              start={event.start}
              onChange={(recurrence) => set({ recurrence })}
            />
          ) : (
            // Said only when the series genuinely cannot be reached: without an
            // identifier from the server there is no way to address it, and
            // offering the choice would be offering something that cannot be
            // carried out.
            event.is_recurring && (
              <p className="rounded-xl bg-raised px-3 py-2 text-[0.6875rem] text-secondary">
                {t('calendar.seriesNotReachable', {
                  defaultValue:
                    'This event repeats, but its series has not been identified yet. Changes here apply to this day only; a refresh usually resolves it.',
                })}
              </p>
            )
          )}

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
        </div>

        {error && (
          <p className="shrink-0 px-5 pt-2 text-[0.6875rem] text-rose-500">{error}</p>
        )}

        {asking && (
          <ScopeAsk
            action={asking}
            onCancel={() => setPendingScope(null)}
            onChoose={(scope: EditScope) => {
              setPendingScope(null)
              if (asking === 'save') void saveEvent(event, scope)
              else void deleteEvent(event, scope)
            }}
          />
        )}

        <div className="flex shrink-0 items-center justify-between gap-2 border-t border-border/60 px-5 py-4">
          {!isNew ? (
            <button
              type="button"
              onClick={() => (repeats ? setPendingScope('delete') : void deleteEvent(event))}
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
              onClick={() => (repeats ? setPendingScope('save') : void saveEvent(event))}
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
/// A date, an hour and a minute, as three controls rather than one
/// `datetime-local`.
///
/// The engine this app runs on renders `datetime-local` with a date picker and
/// no way to reach the time, which left every event stuck at whatever hour it
/// already had. Plain controls work the same everywhere; hour and minute are
/// separate so any time is reachable exactly, without hunting through a list
/// of every minute in the day.
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

  const setDate = (text: string) => {
    const [year, month, day] = text.split('-').map(Number)
    if (!year || !month || !day) return
    const next = new Date(value * 1000)
    next.setFullYear(year, month - 1, day)
    onChange(Math.floor(next.getTime() / 1000))
  }

  const setClock = (hours: number, minutes: number) => {
    const next = new Date(value * 1000)
    next.setHours(hours, minutes, 0, 0)
    onChange(Math.floor(next.getTime() / 1000))
  }

  return (
    <div className="flex items-stretch gap-2">
      <input
        type="date"
        value={dateValue}
        onChange={(e) => setDate(e.target.value)}
        className={`${inputClass} min-w-0 flex-1 max-w-[11.5rem]`}
      />
      {/* An all-day event has no hour to set, so none is offered. Hours and
          minutes share one frame: they are two controls but a single reading,
          and framing them apart made the time look wider than the date it
          belongs to. */}
      {!allDay && (
        <div className="flex shrink-0 items-center rounded-xl border border-border bg-raised px-2 focus-within:border-transparent focus-within:bg-chats focus-within:ring-1 focus-within:ring-accent">
          <select
            value={date.getHours()}
            onChange={(e) => setClock(Number(e.target.value), date.getMinutes())}
            className={clockClass}
            aria-label="hh"
          >
            {HOURS.map((hour) => (
              <option key={hour} value={hour}>
                {pad(hour)}
              </option>
            ))}
          </select>
          <span className="text-xs font-medium text-secondary">:</span>
          <select
            value={date.getMinutes()}
            onChange={(e) => setClock(date.getHours(), Number(e.target.value))}
            className={clockClass}
            aria-label="mm"
          >
            {minuteChoices(date.getMinutes()).map((minute) => (
              <option key={minute} value={minute}>
                {pad(minute)}
              </option>
            ))}
          </select>
        </div>
      )}
    </div>
  )
}

/// The hour and minute selects, bare: the frame around them is the control.
const clockClass =
  'appearance-none bg-transparent px-2 py-2 text-sm font-medium tabular-nums text-primary outline-none cursor-pointer'

/// How an event repeats: the frequency, the days for a weekly rule, and when
/// it stops. Absent unless the reader asks for it, since most events happen
/// once.
function RepeatFields({
  rule,
  start,
  onChange,
}: {
  rule: Recurrence | null
  start: number
  onChange: (rule: Recurrence | null) => void
}) {
  const { t } = useTranslation()
  const startWeekday = (new Date(start * 1000).getDay() + 6) % 7
  const days = rule?.weekdays?.length ? rule.weekdays : [startWeekday]

  const setFreq = (freq: string) => {
    if (freq === '') return onChange(null)
    onChange({
      freq: freq as Frequency,
      interval: rule?.interval ?? 1,
      weekdays: freq === 'weekly' ? days : [],
      until: rule?.until ?? null,
      count: rule?.count ?? null,
    })
  }

  const toggleDay = (day: number) => {
    if (!rule) return
    const next = days.includes(day) ? days.filter((d) => d !== day) : [...days, day].sort()
    // A weekly rule with no day at all falls back to the event's own, which is
    // what the server would do anyway.
    onChange({ ...rule, weekdays: next.length > 0 ? next : [startWeekday] })
  }

  const ends: 'never' | 'on' | 'after' = rule?.until ? 'on' : rule?.count ? 'after' : 'never'

  return (
    <div className="flex flex-col gap-2.5">
      <Labelled label={t('calendar.repeat', { defaultValue: 'Repeats' })}>
        <select value={rule?.freq ?? ''} onChange={(e) => setFreq(e.target.value)} className={inputClass}>
          <option value="">{t('calendar.repeatNever', { defaultValue: 'Does not repeat' })}</option>
          <option value="daily">{t('calendar.repeatDaily', { defaultValue: 'Every day' })}</option>
          <option value="weekly">{t('calendar.repeatWeekly', { defaultValue: 'Every week' })}</option>
          <option value="monthly">{t('calendar.repeatMonthly', { defaultValue: 'Every month' })}</option>
          <option value="yearly">{t('calendar.repeatYearly', { defaultValue: 'Every year' })}</option>
        </select>
      </Labelled>

      {rule && (
        <>
          {rule.freq === 'weekly' && (
            <div className="flex gap-1">
              {WEEKDAY_INITIALS.map((initial, day) => (
                <button
                  key={day}
                  type="button"
                  onClick={() => toggleDay(day)}
                  aria-pressed={days.includes(day)}
                  className={`h-7 w-7 rounded-full text-[0.625rem] font-semibold transition-colors cursor-pointer ${
                    days.includes(day)
                      ? 'bg-accent text-white'
                      : 'bg-raised text-secondary hover:text-primary'
                  }`}
                >
                  {initial}
                </button>
              ))}
            </div>
          )}

          <div className="flex items-center gap-2">
            <span className="text-[0.6875rem] font-semibold text-secondary">
              {t('calendar.repeatEvery', { defaultValue: 'Every' })}
            </span>
            <input
              type="number"
              min={1}
              max={99}
              value={rule.interval}
              onChange={(e) => onChange({ ...rule, interval: Math.max(1, Number(e.target.value)) })}
              className={`${inputClass} w-16`}
            />
            <span className="text-[0.6875rem] text-secondary">{intervalUnit(rule.freq, rule.interval, t)}</span>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <select
              value={ends}
              onChange={(e) => {
                const choice = e.target.value
                if (choice === 'never') onChange({ ...rule, until: null, count: null })
                else if (choice === 'on')
                  onChange({ ...rule, until: start + 30 * 24 * 3600, count: null })
                else onChange({ ...rule, until: null, count: 10 })
              }}
              className={`${inputClass} w-auto`}
            >
              <option value="never">{t('calendar.endsNever', { defaultValue: 'Forever' })}</option>
              <option value="on">{t('calendar.endsOn', { defaultValue: 'Until a date' })}</option>
              <option value="after">{t('calendar.endsAfter', { defaultValue: 'After a number of times' })}</option>
            </select>

            {ends === 'on' && (
              <input
                type="date"
                value={toDateInput(rule.until ?? start)}
                onChange={(e) => onChange({ ...rule, until: fromDateInput(e.target.value, rule.until ?? start) })}
                className={`${inputClass} w-auto`}
              />
            )}
            {ends === 'after' && (
              <div className="flex items-center gap-2">
                <input
                  type="number"
                  min={1}
                  max={999}
                  value={rule.count ?? 10}
                  onChange={(e) => onChange({ ...rule, count: Math.max(1, Number(e.target.value)) })}
                  className={`${inputClass} w-20`}
                />
                <span className="text-[0.6875rem] text-secondary">
                  {t('calendar.times', { defaultValue: 'times' })}
                </span>
              </div>
            )}
          </div>
        </>
      )}
    </div>
  )
}

/// Monday first, matching the weekday numbering used throughout.
const WEEKDAY_INITIALS = ['L', 'M', 'X', 'J', 'V', 'S', 'D']

/// The unit that follows "every N". Singular and plural are separate keys
/// rather than one plural rule: the catalogue reads a lone word in braces as a
/// placeholder, and a word is all these are.
function intervalUnit(freq: Frequency, count: number, t: ReturnType<typeof useTranslation>['t']): string {
  const unit =
    freq === 'daily' ? 'Day' : freq === 'weekly' ? 'Week' : freq === 'monthly' ? 'Month' : 'Year'
  const single = count === 1
  return t(`calendar.unit${unit}${single ? 'One' : 'Many'}`, {
    defaultValue: single ? unit.toLowerCase() : `${unit.toLowerCase()}s`,
  })
}

function toDateInput(epochSeconds: number): string {
  const date = new Date(epochSeconds * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function fromDateInput(text: string, fallback: number): number {
  const [year, month, day] = text.split('-').map(Number)
  if (!year || !month || !day) return fallback
  return Math.floor(new Date(year, month - 1, day, 23, 59, 0, 0).getTime() / 1000)
}

const HOURS = Array.from({ length: 24 }, (_, hour) => hour)
/// Five-minute steps, which is short enough for anything anyone schedules and
/// keeps the list to a dozen entries rather than sixty.
const MINUTE_STEPS = Array.from({ length: 12 }, (_, index) => index * 5)

/// The steps, plus the event's own minute when it falls between them — an
/// invitation at 14:37 must survive being looked at.
function minuteChoices(current: number): number[] {
  return MINUTE_STEPS.includes(current)
    ? MINUTE_STEPS
    : [...MINUTE_STEPS, current].sort((a, b) => a - b)
}


