import { useMemo } from 'react'
import { useValue } from '@legendapp/state/react'
import { MapPin, Repeat } from 'lucide-react'
import { useTranslation } from '../../lib/i18n'
import {
  accountColor,
  calendar$,
  openEvent,
  groupByDay,
  type CalendarEvent,
} from '../../states/calendar'

/// A list of what is coming, grouped by day.
///
/// Deliberately not a grid: a grid answers "what does my month look like",
/// which the month view now does; this one answers "what is next", which is
/// the question a mail client's user asks most.
export function AgendaList({
  onEventMenu,
}: {
  onEventMenu: (x: number, y: number, event: CalendarEvent) => void
}) {
  const { t } = useTranslation()
  const events = useValue(calendar$.events)
  const loading = useValue(calendar$.loading)
  const days = useMemo(() => groupByDay(events), [events])

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      {days.length === 0 && !loading && (
        <p className="mt-8 text-center text-xs text-secondary">
          {t('calendar.empty', { defaultValue: 'Nothing scheduled.' })}
        </p>
      )}
      {days.map(({ day, events }) => (
        <section key={day} className="mb-6">
          <h2 className="sticky top-0 z-10 -mx-5 bg-app/95 px-5 pb-2 pt-1 text-[0.6875rem] font-semibold uppercase tracking-wide text-secondary backdrop-blur">
            {formatDayHeading(day, t)}
          </h2>
          <ul className="flex flex-col gap-1.5">
            {events.map((event) => (
              <EventRow
                key={`${event.accountId}:${event.id}`}
                event={event}
                onContextMenu={(x, y) => onEventMenu(x, y, event)}
              />
            ))}
          </ul>
        </section>
      ))}
    </div>
  )
}

function EventRow({
  event,
  onContextMenu,
}: {
  event: CalendarEvent
  onContextMenu: (x: number, y: number) => void
}) {
  const { t } = useTranslation()
  const color = accountColor(event.accountId)
  return (
    <li
      onClick={() => openEvent(event)}
      onContextMenu={(mouse) => {
        mouse.preventDefault()
        onContextMenu(mouse.clientX, mouse.clientY)
      }}
      className={`flex cursor-pointer items-start gap-3 rounded-xl border border-border bg-raised px-3 py-2.5 transition-colors hover:bg-hover ${
        event.is_cancelled ? 'opacity-55' : ''
      }`}
    >
      <span className="mt-1 h-8 w-1 shrink-0 rounded-full" style={{ backgroundColor: color }} />
      <div className="w-20 shrink-0 pt-0.5 text-[0.6875rem] font-medium tabular-nums text-secondary">
        {event.all_day
          ? t('calendar.allDay', { defaultValue: 'All day' })
          : `${formatTime(event.start)}–${formatTime(event.end)}`}
      </div>
      <div className="min-w-0 flex-1">
        <p
          className={`truncate text-[0.8125rem] font-medium text-primary ${
            event.is_cancelled ? 'line-through' : ''
          }`}
        >
          {event.subject || t('calendar.noSubject', { defaultValue: '(no subject)' })}
        </p>
        <div className="mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[0.6875rem] text-secondary">
          {event.location && (
            <span className="inline-flex min-w-0 items-center gap-1">
              <MapPin size={10} className="shrink-0" />
              <span className="truncate">{event.location}</span>
            </span>
          )}
          {event.is_recurring && (
            <span className="inline-flex items-center gap-1">
              <Repeat size={10} />
              {t('calendar.recurring', { defaultValue: 'Repeats' })}
            </span>
          )}
          {/* The organizer of an internal meeting often has no address the
              server will share, so the name is what identifies them. */}
          {event.organizer?.name && <span className="truncate">{event.organizer.name}</span>}
        </div>
      </div>
    </li>
  )
}

export function formatTime(epochSeconds: number): string {
  return new Date(epochSeconds * 1000).toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  })
}

/// Today and tomorrow read better by name than by date.
function formatDayHeading(day: number, t: ReturnType<typeof useTranslation>['t']): string {
  const date = new Date(day)
  const today = new Date()
  const midnight = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
  const dayDiff = Math.round((day - midnight(today)) / (24 * 3600 * 1000))
  const formatted = date.toLocaleDateString(undefined, {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
  })
  if (dayDiff === 0) return `${t('calendar.today', { defaultValue: 'Today' })} · ${formatted}`
  if (dayDiff === 1) return `${t('calendar.tomorrow', { defaultValue: 'Tomorrow' })} · ${formatted}`
  return formatted
}
