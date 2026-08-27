import { useEffect, useMemo, useRef } from 'react'
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

/// Pixels per hour. Tall enough that a half-hour meeting still fits a line of
/// text, short enough that a working day fits a laptop screen.
const HOUR_PX = 52

/// The week (or a single day) as an hour grid.
///
/// Events sit at their hour and share the column width when they overlap.
/// The strip above the grid holds all-day events, which have no hour to sit
/// at. Clicking an empty slot starts an event there; the first render scrolls
/// to the working morning rather than to midnight.
export function WeekView({
  days,
  onEventMenu,
}: {
  days: 1 | 7
  onEventMenu: (x: number, y: number, event: CalendarEvent) => void
}) {
  const { t } = useTranslation()
  const anchorMs = useValue(calendar$.anchor)
  const events = useValue(calendar$.events)
  const calendars = useValue(calendar$.calendars)
  const scroller = useRef<HTMLDivElement | null>(null)

  const first = days === 7 ? startOfWeek(new Date(anchorMs)) : startOfDay(new Date(anchorMs))
  const columns = useMemo(
    () =>
      Array.from({ length: days }, (_, i) => {
        return new Date(first.getFullYear(), first.getMonth(), first.getDate() + i)
      }),
    [first.getTime(), days],
  )
  const todayMs = startOfDay(new Date()).getTime()

  useEffect(() => {
    scroller.current?.scrollTo({ top: 7.5 * HOUR_PX })
  }, [days, anchorMs])

  const colorOf = (event: CalendarEvent) =>
    calendars.find(
      (calendar) => calendar.accountId === event.accountId && calendar.id === event.calendar_id,
    )?.color || accountColor(event.accountId)

  const allDay = (event: CalendarEvent) => event.all_day || event.end - event.start >= 24 * 3600

  const byColumn = useMemo(
    () =>
      columns.map((date) => {
        const dayStart = date.getTime() / 1000
        const dayEnd = new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1).getTime() / 1000
        const timed = events.filter(
          (event) => !allDay(event) && event.start < dayEnd && event.end > dayStart,
        )
        return { date, dayStart, dayEnd, lanes: placeInLanes(timed) }
      }),
    [columns, events],
  )

  const allDayByColumn = useMemo(
    () =>
      columns.map((date) => {
        const dayStart = date.getTime() / 1000
        const dayEnd = new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1).getTime() / 1000
        return events.filter(
          (event) => allDay(event) && event.start < dayEnd && event.end > dayStart,
        )
      }),
    [columns, events],
  )

  const hours = Array.from({ length: 24 }, (_, hour) => hour)

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Day headings, and the all-day strip beneath them. */}
      <div className="shrink-0 border-b border-border pr-3">
        <div className="grid" style={{ gridTemplateColumns: `3.5rem repeat(${days}, 1fr)` }}>
          <div />
          {columns.map((date) => (
            <button
              key={date.getTime()}
              type="button"
              onClick={() => {
                calendar$.anchor.set(date.getTime())
                setCalendarView('day')
              }}
              className="flex items-baseline justify-center gap-1.5 px-1 py-2 cursor-pointer transition-colors hover:bg-hover"
            >
              <span className="text-[0.625rem] font-semibold uppercase tracking-wide text-secondary">
                {date.toLocaleDateString(undefined, { weekday: 'short' })}
              </span>
              <span
                className={`rounded-md px-1 text-[0.8125rem] font-semibold tabular-nums ${
                  date.getTime() === todayMs ? 'bg-accent text-white' : 'text-primary'
                }`}
              >
                {date.getDate()}
              </span>
            </button>
          ))}
          <div className="row-start-2 pb-1 pr-1 text-right text-[0.5625rem] uppercase tracking-wide text-secondary/70">
            {t('calendar.allDay', { defaultValue: 'All day' })}
          </div>
          {allDayByColumn.map((dayEvents, i) => (
            <div key={columns[i].getTime()} className="row-start-2 flex min-w-0 flex-col gap-0.5 px-0.5 pb-1">
              {dayEvents.map((event) => (
                <button
                  key={`${event.accountId}:${event.id}`}
                  type="button"
                  onClick={() => openEvent(event)}
                  onContextMenu={(mouse) => {
                    mouse.preventDefault()
                    onEventMenu(mouse.clientX, mouse.clientY, event)
                  }}
                  className="truncate rounded px-1.5 py-px text-left text-[0.625rem] font-medium text-white cursor-pointer hover:opacity-90"
                  style={{ backgroundColor: colorOf(event) }}
                >
                  {event.subject || t('calendar.noSubject', { defaultValue: '(no subject)' })}
                </button>
              ))}
            </div>
          ))}
        </div>
      </div>

      {/* The hour grid. */}
      <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto">
        <div className="grid" style={{ gridTemplateColumns: `3.5rem repeat(${days}, 1fr)` }}>
          <div className="relative" style={{ height: 24 * HOUR_PX }}>
            {hours.map((hour) => (
              <span
                key={hour}
                className="absolute right-2 -translate-y-1/2 text-[0.625rem] tabular-nums text-secondary/80"
                style={{ top: hour * HOUR_PX }}
              >
                {hour === 0 ? '' : `${String(hour).padStart(2, '0')}:00`}
              </span>
            ))}
          </div>
          {byColumn.map(({ date, dayStart, lanes }) => (
            <div
              key={date.getTime()}
              className="relative border-l border-border/60"
              style={{ height: 24 * HOUR_PX }}
              onDoubleClick={(mouse) => {
                const bounds = mouse.currentTarget.getBoundingClientRect()
                const hour = Math.floor((mouse.clientY - bounds.top) / HOUR_PX)
                newEvent(dayStart + hour * 3600)
              }}
            >
              {hours.map((hour) => (
                <div
                  key={hour}
                  className="absolute inset-x-0 border-t border-border/40"
                  style={{ top: hour * HOUR_PX }}
                />
              ))}
              {lanes.map(({ event, lane, laneCount }) => {
                const top = ((Math.max(event.start, dayStart) - dayStart) / 3600) * HOUR_PX
                const bottom =
                  ((Math.min(event.end, dayStart + 24 * 3600) - dayStart) / 3600) * HOUR_PX
                const color = colorOf(event)
                return (
                  <button
                    key={`${event.accountId}:${event.id}`}
                    type="button"
                    onClick={() => openEvent(event)}
                    onContextMenu={(mouse) => {
                      mouse.preventDefault()
                      onEventMenu(mouse.clientX, mouse.clientY, event)
                    }}
                    className={`absolute overflow-hidden rounded-md border-l-2 px-1.5 py-0.5 text-left text-[0.625rem] leading-tight transition-opacity hover:opacity-90 cursor-pointer ${
                      event.is_cancelled ? 'line-through opacity-55' : ''
                    }`}
                    style={{
                      top,
                      height: Math.max(bottom - top, 20),
                      left: `calc(${(lane / laneCount) * 100}% + 2px)`,
                      width: `calc(${100 / laneCount}% - 4px)`,
                      backgroundColor: `${color}26`,
                      borderLeftColor: color,
                    }}
                  >
                    <span className="block truncate font-medium text-primary">
                      {event.subject || t('calendar.noSubject', { defaultValue: '(no subject)' })}
                    </span>
                    <span className="block truncate tabular-nums text-secondary">
                      {formatTime(event.start)}–{formatTime(event.end)}
                    </span>
                  </button>
                )
              })}
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

/// Assigns overlapping events to side-by-side lanes, greedily: each event
/// takes the first lane free at its start. Events in one overlap cluster share
/// the column width evenly.
function placeInLanes(
  events: CalendarEvent[],
): { event: CalendarEvent; lane: number; laneCount: number }[] {
  const sorted = [...events].sort((a, b) => a.start - b.start || b.end - a.end)
  const placed: { event: CalendarEvent; lane: number; laneCount: number }[] = []
  let cluster: { event: CalendarEvent; lane: number; laneCount: number }[] = []
  let laneEnds: number[] = []
  let clusterEnd = -Infinity

  const closeCluster = () => {
    for (const entry of cluster) entry.laneCount = laneEnds.length
    placed.push(...cluster)
    cluster = []
    laneEnds = []
  }

  for (const event of sorted) {
    if (cluster.length > 0 && event.start >= clusterEnd) closeCluster()
    let lane = laneEnds.findIndex((end) => end <= event.start)
    if (lane === -1) {
      lane = laneEnds.length
      laneEnds.push(event.end)
    } else {
      laneEnds[lane] = event.end
    }
    cluster.push({ event, lane, laneCount: 0 })
    clusterEnd = Math.max(clusterEnd, event.end)
  }
  closeCluster()
  return placed
}
