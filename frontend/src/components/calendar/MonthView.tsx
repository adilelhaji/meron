import { useMemo } from 'react'
import { useValue } from '@legendapp/state/react'
import { useTranslation } from '../../lib/i18n'
import {
  accountColor,
  calendar$,
  openEvent,
  newEvent,
  setCalendarView,
  startOfDay,
  startOfWeek,
  type CalendarEvent,
} from '../../states/calendar'
import { formatTime } from './AgendaView'

/// How many events a month cell names before folding the rest into a count.
const CELL_EVENTS = 3

/// The month as a six-week grid, padding days included.
///
/// A cell names its first few events and counts the rest: a month view is for
/// seeing the shape of the month, and the day and week views are one click
/// away for anything it cannot fit. Clicking a day number opens that day;
/// double-clicking a cell's empty space starts an event that morning.
export function MonthView({
  onEventMenu,
}: {
  onEventMenu: (x: number, y: number, event: CalendarEvent) => void
}) {
  const { t } = useTranslation()
  const anchorMs = useValue(calendar$.anchor)
  const events = useValue(calendar$.events)
  const calendars = useValue(calendar$.calendars)

  const anchor = new Date(anchorMs)
  const gridStart = startOfWeek(new Date(anchor.getFullYear(), anchor.getMonth(), 1))
  const todayMs = startOfDay(new Date()).getTime()

  const days = useMemo(
    () =>
      Array.from({ length: 42 }, (_, i) => {
        const date = new Date(gridStart.getFullYear(), gridStart.getMonth(), gridStart.getDate() + i)
        return date
      }),
    [gridStart.getTime()],
  )

  const byDay = useMemo(() => {
    const map = new Map<number, CalendarEvent[]>()
    for (const event of events) {
      const start = new Date(event.start * 1000)
      const key = startOfDay(start).getTime()
      const bucket = map.get(key)
      if (bucket) bucket.push(event)
      else map.set(key, [event])
    }
    for (const bucket of map.values()) bucket.sort((a, b) => a.start - b.start)
    return map
  }, [events])

  const colorOf = (event: CalendarEvent) =>
    calendars.find(
      (calendar) => calendar.accountId === event.accountId && calendar.id === event.calendar_id,
    )?.color || accountColor(event.accountId)

  const openDay = (date: Date) => {
    calendar$.anchor.set(date.getTime())
    setCalendarView('day')
  }

  const weekdays = useMemo(
    () =>
      Array.from({ length: 7 }, (_, i) => {
        const date = new Date(gridStart.getFullYear(), gridStart.getMonth(), gridStart.getDate() + i)
        return date.toLocaleDateString(undefined, { weekday: 'short' })
      }),
    [gridStart.getTime()],
  )

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="grid shrink-0 grid-cols-7 border-b border-border">
        {weekdays.map((name) => (
          <div
            key={name}
            className="px-2 py-1.5 text-center text-[0.625rem] font-semibold uppercase tracking-wide text-secondary"
          >
            {name}
          </div>
        ))}
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-7 grid-rows-6">
        {days.map((date) => {
          const key = date.getTime()
          const inMonth = date.getMonth() === anchor.getMonth()
          const isToday = key === todayMs
          const dayEvents = byDay.get(key) ?? []
          const overflow = dayEvents.length - CELL_EVENTS
          return (
            <div
              key={key}
              onDoubleClick={() => {
                const at = new Date(date.getFullYear(), date.getMonth(), date.getDate(), 9)
                newEvent(Math.floor(at.getTime() / 1000))
              }}
              className={`flex min-h-0 flex-col gap-0.5 overflow-hidden border-b border-r border-border/60 p-1 ${
                inMonth ? '' : 'bg-raised/40'
              }`}
            >
              <button
                type="button"
                onClick={() => openDay(date)}
                className={`self-start rounded-md px-1.5 py-0.5 text-[0.6875rem] font-semibold tabular-nums cursor-pointer transition-colors ${
                  isToday
                    ? 'bg-accent text-white'
                    : inMonth
                      ? 'text-primary hover:bg-hover'
                      : 'text-secondary/60 hover:bg-hover'
                }`}
              >
                {date.getDate()}
              </button>
              {dayEvents.slice(0, CELL_EVENTS).map((event) => (
                <button
                  key={`${event.accountId}:${event.id}`}
                  type="button"
                  onClick={() => openEvent(event)}
                  onContextMenu={(mouse) => {
                    mouse.preventDefault()
                    onEventMenu(mouse.clientX, mouse.clientY, event)
                  }}
                  className={`flex min-w-0 items-center gap-1 rounded px-1 py-px text-left text-[0.625rem] leading-4 text-primary transition-colors hover:bg-hover cursor-pointer ${
                    event.is_cancelled ? 'line-through opacity-55' : ''
                  }`}
                >
                  <span
                    className="h-1.5 w-1.5 shrink-0 rounded-full"
                    style={{ backgroundColor: colorOf(event) }}
                  />
                  {!event.all_day && (
                    <span className="shrink-0 tabular-nums text-secondary">
                      {formatTime(event.start)}
                    </span>
                  )}
                  <span className="truncate">
                    {event.subject || t('calendar.noSubject', { defaultValue: '(no subject)' })}
                  </span>
                </button>
              ))}
              {overflow > 0 && (
                <button
                  type="button"
                  onClick={() => openDay(date)}
                  className="self-start rounded px-1 text-[0.625rem] font-medium text-accent hover:bg-accent/10 cursor-pointer"
                >
                  {t('calendar.moreEvents', { defaultValue: '{count} more', count: overflow })}
                </button>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}
