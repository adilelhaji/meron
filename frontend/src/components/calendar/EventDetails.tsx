import { useValue } from '@legendapp/state/react'
import { CalendarDays, Clock, MapPin, Repeat, SquarePen, Trash2, User, Users, X } from 'lucide-react'
import { useTranslation } from '../../lib/i18n'
import { useEscapeKey } from '../../lib/useEscapeKey'
import {
  calendar$,
  eventColor,
  closeDetails,
  deleteEvent,
  editEvent,
  type CalendarEvent,
} from '../../states/calendar'

/// An event at full size, to read rather than to edit.
///
/// Opening an event used to drop the reader straight into the editor, which
/// answers "change this" when the question is almost always "what is this".
/// Editing is one click away, and refused outright on a subscribed calendar,
/// which belongs to whoever publishes it.
export function EventDetails() {
  const { t } = useTranslation()
  const event = useValue(calendar$.viewing)
  const calendars = useValue(calendar$.calendars)
  useEscapeKey(closeDetails, Boolean(event))
  if (!event) return null

  const calendar = calendars.find(
    (candidate) => candidate.accountId === event.accountId && candidate.id === event.calendar_id,
  )
  const color = eventColor(event, calendars)
  const readOnly = Boolean(calendar?.read_only)

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4"
      onMouseDown={(mouse) => {
        if (mouse.target === mouse.currentTarget) closeDetails()
      }}
    >
      <div className="max-h-[85vh] w-full max-w-lg overflow-y-auto rounded-2xl border border-border bg-app shadow-xl">
        {/* A band in the calendar's colour, so which calendar this belongs to
            is answered before the text is read. */}
        <div className="h-1.5 rounded-t-2xl" style={{ backgroundColor: color }} />

        <div className="flex items-start gap-3 px-5 pb-3 pt-4">
          <h2
            className={`min-w-0 flex-1 text-lg font-semibold leading-snug text-primary ${
              event.is_cancelled ? 'line-through opacity-60' : ''
            }`}
          >
            {event.subject || t('calendar.noSubject', { defaultValue: '(no subject)' })}
          </h2>
          <button
            type="button"
            onClick={closeDetails}
            aria-label={t('calendar.close', { defaultValue: 'Close' })}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-secondary hover:bg-hover hover:text-primary cursor-pointer"
          >
            <X size={15} />
          </button>
        </div>

        <div className="flex flex-col gap-3 px-5 pb-4">
          <Row icon={<Clock size={14} />}>
            <span className="text-primary">{formatRange(event)}</span>
            {/* How far off and how long: the two questions asked of a date
                that the date itself does not answer. */}
            <span className="mt-0.5 text-secondary">
              {[relativeWhen(event, t), formatDuration(event, t)].filter(Boolean).join(' · ')}
            </span>
            {event.is_recurring && (
              <span className="mt-0.5 flex items-center gap-1 text-secondary">
                <Repeat size={11} />
                {t('calendar.recurring', { defaultValue: 'Repeats' })}
              </span>
            )}
          </Row>

          {event.location && (
            <Row icon={<MapPin size={14} />}>
              <span className="text-primary">{event.location}</span>
            </Row>
          )}

          <Row icon={<CalendarDays size={14} />}>
            <span className="flex items-center gap-1.5 text-primary">
              <span
                className="h-2.5 w-2.5 shrink-0 rounded-full"
                style={{ backgroundColor: color }}
              />
              {calendar?.name ?? t('calendar.calendar', { defaultValue: 'Calendar' })}
            </span>
          </Row>

          {event.organizer && (
            <Row icon={<User size={14} />}>
              <span className="text-primary">
                {event.organizer.name || event.organizer.addr}
              </span>
              <span className="text-secondary">
                {t('calendar.organizer', { defaultValue: 'Organiser' })}
              </span>
            </Row>
          )}

          {event.attendees.length > 0 && (
            <Row icon={<Users size={14} />}>
              <ul className="flex flex-col gap-1">
                {event.attendees.map((person, index) => (
                  <li key={`${person.addr}:${index}`} className="flex items-baseline gap-2">
                    {/* The name is what identifies someone here: an internal
                        directory often gives no address the server will share. */}
                    <span className="min-w-0 truncate text-primary">
                      {person.name || person.addr}
                    </span>
                    {person.response && (
                      <span className="shrink-0 text-[0.625rem] text-secondary">
                        {responseLabel(person.response, t)}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </Row>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-border/60 px-5 py-3">
          {readOnly ? (
            <p className="mr-auto text-[0.6875rem] text-secondary">
              {t('calendar.readOnlyEvent', {
                defaultValue: 'This calendar is read-only — it belongs to whoever publishes it.',
              })}
            </p>
          ) : (
            <button
              type="button"
              onClick={() => void deleteEvent(event)}
              className="mr-auto inline-flex items-center gap-1.5 rounded-xl px-3 py-2 text-xs font-medium text-rose-500 transition-colors hover:bg-rose-500/10 cursor-pointer"
            >
              <Trash2 size={13} />
              {t('calendar.delete', { defaultValue: 'Delete' })}
            </button>
          )}
          <button
            type="button"
            onClick={closeDetails}
            className="rounded-xl px-3 py-2 text-xs font-medium text-secondary transition-colors hover:bg-hover hover:text-primary cursor-pointer"
          >
            {t('calendar.close', { defaultValue: 'Close' })}
          </button>
          {!readOnly && (
            <button
              type="button"
              onClick={() => editEvent(event)}
              className="inline-flex items-center gap-1.5 rounded-xl bg-accent px-4 py-2 text-xs font-semibold text-white transition-opacity hover:opacity-90 cursor-pointer"
            >
              <SquarePen size={13} />
              {t('calendar.editEvent', { defaultValue: 'Edit event' })}
            </button>
          )}
        </div>
      </div>
    </div>
  )
}

function Row({ icon, children }: { icon: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="flex items-start gap-3 text-xs">
      <span className="mt-0.5 shrink-0 text-secondary">{icon}</span>
      <div className="flex min-w-0 flex-col">{children}</div>
    </div>
  )
}

/// The whole span in words: one line when it starts and ends the same day.
function formatRange(event: CalendarEvent): string {
  const start = new Date(event.start * 1000)
  const end = new Date(event.end * 1000)
  const day = (date: Date) =>
    date.toLocaleDateString(undefined, {
      weekday: 'long',
      day: 'numeric',
      month: 'long',
      year: 'numeric',
    })
  const time = (date: Date) =>
    date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })

  if (event.all_day) return day(start)
  const sameDay = start.toDateString() === end.toDateString()
  return sameDay
    ? `${day(start)} · ${time(start)}–${time(end)}`
    : `${day(start)} ${time(start)} → ${day(end)} ${time(end)}`
}

/// How far off it is, in the coarsest unit that still says something: days
/// once it is a day away, hours below that, and nothing at all for something
/// happening now.
function relativeWhen(
  event: CalendarEvent,
  t: ReturnType<typeof useTranslation>['t'],
): string {
  const now = Date.now() / 1000
  if (event.end < now) {
    const days = Math.round((now - event.end) / 86400)
    if (days >= 1) return t('calendar.daysAgo', { defaultValue: '{count} days ago', count: days })
    return t('calendar.past', { defaultValue: 'Already over' })
  }
  if (event.start <= now) return t('calendar.happeningNow', { defaultValue: 'Happening now' })

  const seconds = event.start - now
  const days = Math.round(seconds / 86400)
  if (days >= 1) {
    if (days === 1) return t('calendar.tomorrow', { defaultValue: 'Tomorrow' })
    return t('calendar.inDays', { defaultValue: 'In {count} days', count: days })
  }
  const hours = Math.floor(seconds / 3600)
  if (hours >= 1) return t('calendar.inHours', { defaultValue: 'In {count} h', count: hours })
  const minutes = Math.max(1, Math.round(seconds / 60))
  return t('calendar.inMinutes', { defaultValue: 'In {count} min', count: minutes })
}

/// How long it lasts. An all-day event is counted in days, since hours are not
/// what it was written in.
function formatDuration(
  event: CalendarEvent,
  t: ReturnType<typeof useTranslation>['t'],
): string {
  const seconds = Math.max(0, event.end - event.start)
  if (event.all_day || seconds >= 86400) {
    const days = Math.max(1, Math.round(seconds / 86400))
    return t('calendar.lastsDays', { defaultValue: '{count} days', count: days })
  }
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.round((seconds % 3600) / 60)
  if (hours === 0) return t('calendar.lastsMinutes', { defaultValue: '{count} min', count: minutes })
  if (minutes === 0) return t('calendar.lastsHours', { defaultValue: '{count} h', count: hours })
  return t('calendar.lastsHoursMinutes', {
    defaultValue: '{hours} h {minutes} min',
    hours,
    minutes,
  })
}

/// Servers answer with their own vocabulary; these are the three answers that
/// mean something to a reader.
function responseLabel(response: string, t: ReturnType<typeof useTranslation>['t']): string {
  const normalised = response.toLowerCase()
  if (normalised.includes('accept')) return t('calendar.accepted', { defaultValue: 'Accepted' })
  if (normalised.includes('decline')) return t('calendar.declined', { defaultValue: 'Declined' })
  if (normalised.includes('tentative')) return t('calendar.tentative', { defaultValue: 'Tentative' })
  return t('calendar.noAnswer', { defaultValue: 'No answer yet' })
}
