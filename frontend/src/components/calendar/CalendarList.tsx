import { useState } from 'react'
import { useValue } from '@legendapp/state/react'
import { Check, Lock } from 'lucide-react'
import { useTranslation } from '../../lib/i18n'
import { accounts$ } from '../../states/accounts'
import {
  CALENDAR_COLORS,
  accountColor,
  calendar$,
  setCalendarColor,
  setCalendarEnabled,
  type Calendar,
} from '../../states/calendar'

/// The calendars on the left of the calendar, the way every calendar app puts
/// them: one row each, a tick that shows or hides it, and its colour — which
/// can be changed here, since this is where the reader is looking when they
/// decide two calendars are too alike to tell apart.
///
/// Grouped by account, because that is what decides where a calendar lives and
/// how it syncs.
export function CalendarList() {
  const { t } = useTranslation()
  const calendars = useValue(calendar$.calendars)
  const accounts = useValue(accounts$)
  const [picking, setPicking] = useState<string | null>(null)

  if (calendars.length === 0) return null

  const groups = accounts
    .map((account) => ({
      account,
      calendars: calendars.filter((calendar) => calendar.accountId === account.id),
    }))
    .filter((group) => group.calendars.length > 0)

  const key = (calendar: Calendar) => `${calendar.accountId}:${calendar.id}`

  return (
    <aside className="hidden w-56 shrink-0 flex-col gap-4 overflow-y-auto border-r border-border bg-raised/40 px-3 py-4 md:flex">
      {groups.map(({ account, calendars }) => (
        <div key={account.id}>
          <p className="mb-1.5 truncate px-1 text-[0.625rem] font-semibold uppercase tracking-wide text-secondary/70">
            {account.email}
          </p>
          <ul className="flex flex-col">
            {calendars.map((calendar) => {
              const color = calendar.color || accountColor(calendar.accountId)
              const open = picking === key(calendar)
              return (
                <li key={key(calendar)} className="relative">
                  <div className="flex items-center gap-2 rounded-lg px-1 py-1.5 transition-colors hover:bg-hover">
                    {/* The tick doubles as the colour: one square answers both
                        "is it shown" and "which one is it" at a glance. */}
                    <button
                      type="button"
                      onClick={() =>
                        void setCalendarEnabled(
                          calendar.accountId,
                          calendar.id,
                          !calendar.enabled,
                        )
                      }
                      aria-label={calendar.name}
                      aria-pressed={calendar.enabled}
                      className="flex h-4 w-4 shrink-0 items-center justify-center rounded transition-colors cursor-pointer"
                      style={{
                        backgroundColor: calendar.enabled ? color : 'transparent',
                        border: `1.5px solid ${color}`,
                      }}
                    >
                      {calendar.enabled && <Check size={11} className="text-white" strokeWidth={3} />}
                    </button>
                    <button
                      type="button"
                      onClick={() => setPicking(open ? null : key(calendar))}
                      title={t('calendar.color', { defaultValue: 'Colour' })}
                      className={`min-w-0 flex-1 truncate text-left text-xs transition-colors cursor-pointer ${
                        calendar.enabled ? 'text-primary' : 'text-secondary'
                      }`}
                    >
                      {calendar.name}
                    </button>
                    {calendar.read_only && (
                      <Lock size={10} className="shrink-0 text-secondary/70" />
                    )}
                  </div>

                  {open && (
                    <>
                      <div className="fixed inset-0 z-40" onClick={() => setPicking(null)} />
                      <div className="absolute left-6 top-8 z-50 flex gap-1.5 rounded-xl border border-border bg-app p-2 shadow-xl">
                        {CALENDAR_COLORS.map((swatch) => (
                          <button
                            key={swatch}
                            type="button"
                            aria-label={swatch}
                            onClick={() => {
                              void setCalendarColor(calendar.accountId, calendar.id, swatch)
                              setPicking(null)
                            }}
                            className={`h-5 w-5 rounded-full transition-transform hover:scale-110 cursor-pointer ${
                              swatch === color ? 'ring-2 ring-primary/40 ring-offset-2 ring-offset-app' : ''
                            }`}
                            style={{ backgroundColor: swatch }}
                          />
                        ))}
                      </div>
                    </>
                  )}
                </li>
              )
            })}
          </ul>
        </div>
      ))}
    </aside>
  )
}
