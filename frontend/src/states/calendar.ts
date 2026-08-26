import { observable } from '@legendapp/state'
import { invoke } from '../lib/bridge'
import { accounts$ } from './accounts'

/// One person on an event. `addr` is empty when the server identified them
/// only by an internal directory name, which is why `name` is what gets shown.
export type Participant = {
  name: string
  addr: string
  response: string
}

/// One occurrence, never a series: the server expands recurrences over the
/// window we ask for, so every event here is a concrete instance with real
/// start and end instants.
export type CalendarEvent = {
  id: string
  calendar_id: string
  change_key?: string | null
  subject: string
  location?: string | null
  /// Epoch seconds.
  start: number
  end: number
  all_day: boolean
  is_recurring: boolean
  is_cancelled: boolean
  free_busy: string
  my_response: string
  organizer?: Participant | null
  attendees: Participant[]
  /// Which account it came from. The core answers per account, so this is
  /// stamped on merge rather than sent.
  accountId: string
}

export type Calendar = {
  id: string
  name: string
  is_default: boolean
  enabled: boolean
  color?: string | null
  accountId: string
}

export const calendar$ = observable({
  /// The window currently shown, as epoch seconds.
  from: 0,
  to: 0,
  events: [] as CalendarEvent[],
  calendars: [] as Calendar[],
  loading: false,
  error: '',
})

/// Colours calendars by account so a merged agenda stays readable. Indexed by
/// the account's position, which is stable for a given set of accounts.
const ACCOUNT_COLORS = ['#2056DD', '#E8830C', '#2E9E5B', '#8B5CF6', '#E24C3B', '#0891B2']

export function accountColor(accountId: string): string {
  const index = accounts$.peek().findIndex((account) => account.id === accountId)
  return ACCOUNT_COLORS[(index < 0 ? 0 : index) % ACCOUNT_COLORS.length]
}

/// Loads a window across every account, merged and sorted by start.
///
/// Each account answers from its own cache and refreshes behind the request,
/// so this returns quickly and the view settles when `calendar.synced` lands.
export async function loadWindow(from: number, to: number, refresh = true) {
  calendar$.from.set(from)
  calendar$.to.set(to)
  calendar$.loading.set(true)
  calendar$.error.set('')
  try {
    const accounts = accounts$.peek().filter((account) => !isFeedAccount(account.id))
    const perAccount = await Promise.all(
      accounts.map(async (account) => {
        try {
          const res = await invoke<{ events: CalendarEvent[] }>('calendar.events', {
            account_id: account.id,
            from,
            to,
            refresh,
          })
          return (res.events ?? []).map((event) => ({ ...event, accountId: account.id }))
        } catch {
          // One account failing must not blank the whole agenda; the others
          // still have events worth showing.
          return [] as CalendarEvent[]
        }
      }),
    )
    const merged = perAccount.flat().sort((a, b) => a.start - b.start || a.subject.localeCompare(b.subject))
    calendar$.events.set(merged)
  } catch (err) {
    calendar$.error.set(String(err))
  } finally {
    calendar$.loading.set(false)
  }
}

export async function loadCalendars() {
  const accounts = accounts$.peek().filter((account) => !isFeedAccount(account.id))
  const perAccount = await Promise.all(
    accounts.map(async (account) => {
      try {
        const res = await invoke<{ calendars: Calendar[] }>('calendar.list', {
          account_id: account.id,
        })
        return (res.calendars ?? []).map((calendar) => ({ ...calendar, accountId: account.id }))
      } catch {
        return [] as Calendar[]
      }
    }),
  )
  calendar$.calendars.set(perAccount.flat())
}

export async function setCalendarEnabled(accountId: string, calendarId: string, enabled: boolean) {
  await invoke('calendar.setEnabled', {
    account_id: accountId,
    calendar_id: calendarId,
    enabled,
  })
  calendar$.calendars.set(
    calendar$.calendars
      .peek()
      .map((calendar) =>
        calendar.accountId === accountId && calendar.id === calendarId ? { ...calendar, enabled } : calendar,
      ),
  )
  // The window's contents depend on which calendars are shown, so re-read it.
  const { from, to } = calendar$.peek()
  if (to > from) await loadWindow(from, to, false)
}

function isFeedAccount(id: string): boolean {
  return id.startsWith('rss:') || id === 'unified'
}

/// Groups a window's events by local day, for an agenda that reads as a diary.
export function groupByDay(events: CalendarEvent[]): { day: number; events: CalendarEvent[] }[] {
  const days = new Map<number, CalendarEvent[]>()
  for (const event of events) {
    // Local midnight of the day it starts: an agenda is read in the reader's
    // own timezone, not the server's.
    const start = new Date(event.start * 1000)
    const day = new Date(start.getFullYear(), start.getMonth(), start.getDate()).getTime()
    const bucket = days.get(day)
    if (bucket) bucket.push(event)
    else days.set(day, [event])
  }
  return [...days.entries()]
    .sort(([a], [b]) => a - b)
    .map(([day, events]) => ({ day, events }))
}
