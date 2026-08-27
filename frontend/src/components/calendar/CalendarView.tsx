import { useEffect, useState } from 'react'
import { useValue } from '@legendapp/state/react'
import { CalendarDays, ChevronLeft, ChevronRight, Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from '../../lib/i18n'
import {
  calendar$,
  loadCalendars,
  loadCurrentView,
  navigateCalendar,
  newEvent,
  setCalendarView,
  startOfWeek,
  type CalendarEvent,
  type CalendarViewMode,
} from '../../states/calendar'
import { AgendaList } from './AgendaView'
import { MonthView } from './MonthView'
import { WeekView } from './WeekView'
import { EventEditor } from './EventEditor'
import { EventContextMenu, type EventContextMenuState } from './EventContextMenu'

/// The calendar surface: one header — period, navigation, view switcher — over
/// whichever view is chosen. The editor and the context menu live here so
/// every view gets them for free.
export function CalendarView() {
  const { t } = useTranslation()
  const view = useValue(calendar$.view)
  const anchor = useValue(calendar$.anchor)
  const loading = useValue(calendar$.loading)
  const [menu, setMenu] = useState<EventContextMenuState | null>(null)

  useEffect(() => {
    void loadCalendars()
    void loadCurrentView()
  }, [])

  const onEventMenu = (x: number, y: number, event: CalendarEvent) => setMenu({ x, y, event })

  const views: { mode: CalendarViewMode; label: string }[] = [
    { mode: 'agenda', label: t('calendar.viewAgenda', { defaultValue: 'Agenda' }) },
    { mode: 'day', label: t('calendar.viewDay', { defaultValue: 'Day' }) },
    { mode: 'week', label: t('calendar.viewWeek', { defaultValue: 'Week' }) },
    { mode: 'month', label: t('calendar.viewMonth', { defaultValue: 'Month' }) },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-app">
      <EventEditor />
      {menu && <EventContextMenu state={menu} onClose={() => setMenu(null)} />}
      <header className="flex items-center gap-2 border-b border-border px-5 py-3">
        <CalendarDays size={17} className="shrink-0 text-accent" />
        <h1 className="min-w-0 truncate text-sm font-semibold text-primary">
          {periodLabel(view, anchor, t)}
        </h1>
        {loading && <RefreshCw size={13} className="shrink-0 animate-spin text-secondary" />}

        <div className="ml-auto flex items-center gap-2">
          {view !== 'agenda' && (
            <div className="flex items-center gap-0.5">
              <NavButton label="‹" onClick={() => navigateCalendar(-1)}>
                <ChevronLeft size={14} />
              </NavButton>
              <button
                type="button"
                onClick={() => navigateCalendar(0)}
                className="rounded-lg px-2 py-1 text-[0.6875rem] font-medium text-secondary transition-colors hover:bg-hover hover:text-primary cursor-pointer"
              >
                {t('calendar.today', { defaultValue: 'Today' })}
              </button>
              <NavButton label="›" onClick={() => navigateCalendar(1)}>
                <ChevronRight size={14} />
              </NavButton>
            </div>
          )}

          <div className="flex items-center gap-0.5 rounded-xl border border-border/80 bg-raised p-0.5">
            {views.map(({ mode, label }) => (
              <button
                key={mode}
                type="button"
                onClick={() => setCalendarView(mode)}
                className={`rounded-lg px-2.5 py-1 text-[0.6875rem] font-medium transition-colors cursor-pointer ${
                  view === mode
                    ? 'bg-chats text-primary shadow-sm ring-1 ring-border/80'
                    : 'text-secondary hover:text-primary'
                }`}
              >
                {label}
              </button>
            ))}
          </div>

          <button
            type="button"
            onClick={() => newEvent()}
            className="inline-flex items-center gap-1.5 rounded-xl bg-accent px-3 py-1.5 text-[0.6875rem] font-semibold text-white transition-opacity hover:opacity-90 cursor-pointer"
          >
            <Plus size={13} />
            {t('calendar.newEvent', { defaultValue: 'New event' })}
          </button>
        </div>
      </header>

      {view === 'agenda' && <AgendaList onEventMenu={onEventMenu} />}
      {view === 'month' && <MonthView onEventMenu={onEventMenu} />}
      {view === 'week' && <WeekView days={7} onEventMenu={onEventMenu} />}
      {view === 'day' && <WeekView days={1} onEventMenu={onEventMenu} />}
    </div>
  )
}

function NavButton({
  label,
  onClick,
  children,
}: {
  label: string
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className="flex h-6 w-6 items-center justify-center rounded-lg text-secondary transition-colors hover:bg-hover hover:text-primary cursor-pointer"
    >
      {children}
    </button>
  )
}

/// What the header calls the period on screen.
function periodLabel(
  view: CalendarViewMode,
  anchorMs: number,
  t: ReturnType<typeof useTranslation>['t'],
): string {
  const anchor = new Date(anchorMs)
  if (view === 'month') {
    const label = anchor.toLocaleDateString(undefined, { month: 'long', year: 'numeric' })
    return label.charAt(0).toUpperCase() + label.slice(1)
  }
  if (view === 'week') {
    const monday = startOfWeek(anchor)
    const sunday = new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + 6)
    const sameMonth = monday.getMonth() === sunday.getMonth()
    const from = monday.toLocaleDateString(undefined, {
      day: 'numeric',
      month: sameMonth ? undefined : 'short',
    })
    const to = sunday.toLocaleDateString(undefined, {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    })
    return `${from} – ${to}`
  }
  if (view === 'day') {
    const label = anchor.toLocaleDateString(undefined, {
      weekday: 'long',
      day: 'numeric',
      month: 'long',
      year: 'numeric',
    })
    return label.charAt(0).toUpperCase() + label.slice(1)
  }
  return t('calendar.title', { defaultValue: 'Calendar' })
}
