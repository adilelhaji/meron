import { useState } from 'react'
import { useValue } from '@legendapp/state/react'
import { CalendarDays, Trash2 } from 'lucide-react'
import { useTranslation } from '../../lib/i18n'
import {
  CALENDAR_COLORS,
  calendar$,
  deleteCalendar,
  renameCalendar,
  setCalendarColor,
  setCalendarEnabled,
  type Calendar,
} from '../../states/calendar'
import { SettingsGroup, SettingRow, ToggleRow } from './AccountSettingsRows'
import { Field } from '../field/Field'

/// Settings for one calendar: what it is called, what colour it is drawn in,
/// whether it shows, and removing it.
export function CalendarPanel({ calendar }: { calendar: Calendar }) {
  const { t } = useTranslation()
  const [name, setName] = useState(calendar.name)
  const [confirming, setConfirming] = useState(false)
  const [error, setError] = useState('')
  const calendars = useValue(calendar$.calendars)
  const color = calendar.color || CALENDAR_COLORS[0]

  const renamed = name.trim() && name.trim() !== calendar.name
  const onlyOne = calendars.filter((c) => c.accountId === calendar.accountId).length === 1

  const run = async (action: Promise<unknown>) => {
    setError('')
    try {
      await action
    } catch (err) {
      setError(String(err))
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <SettingsGroup title={t('calendar.calendar', { defaultValue: 'Calendar' })}>
        <div className="flex items-end gap-2 px-4 py-3.5">
          <Field
            label={t('calendar.name', { defaultValue: 'Name' })}
            value={name}
            onChange={setName}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && renamed) {
                void run(renameCalendar(calendar.accountId, calendar.id, name.trim()))
              }
            }}
          />
          <button
            type="button"
            disabled={!renamed}
            onClick={() => void run(renameCalendar(calendar.accountId, calendar.id, name.trim()))}
            className="mb-0.5 shrink-0 rounded-xl bg-accent px-3 py-2 text-xs font-semibold text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40 cursor-pointer"
          >
            {t('calendar.rename', { defaultValue: 'Rename' })}
          </button>
        </div>

        <SettingRow
          icon={<CalendarDays size={15} style={{ color }} />}
          title={t('calendar.color', { defaultValue: 'Colour' })}
          hint={t('calendar.colorHint', {
            defaultValue: 'Shown in this copy of Meron only.',
          })}
          control={
            <div className="flex items-center gap-1.5">
              {CALENDAR_COLORS.map((swatch) => (
                <button
                  key={swatch}
                  type="button"
                  aria-label={swatch}
                  onClick={() => void run(setCalendarColor(calendar.accountId, calendar.id, swatch))}
                  className={`h-5 w-5 rounded-full transition-transform hover:scale-110 cursor-pointer ${
                    swatch === color ? 'ring-2 ring-offset-2 ring-offset-raised ring-primary/40' : ''
                  }`}
                  style={{ backgroundColor: swatch }}
                />
              ))}
            </div>
          }
        />

        <ToggleRow
          title={t('calendar.showInAgenda', { defaultValue: 'Show in the agenda' })}
          hint={t('calendar.showInAgendaHint', {
            defaultValue: 'A hidden calendar is not fetched at all.',
          })}
          checked={calendar.enabled}
          onChange={() =>
            void run(setCalendarEnabled(calendar.accountId, calendar.id, !calendar.enabled))
          }
        />
      </SettingsGroup>

      <SettingsGroup title={t('calendar.dangerZone', { defaultValue: 'Remove' })}>
        <div className="px-4 py-3.5">
          {calendar.is_default ? (
            <p className="text-[0.6875rem] text-secondary">
              {t('calendar.cannotDeleteDefault', {
                defaultValue:
                  "The account's main calendar cannot be removed — the server does not allow it.",
              })}
            </p>
          ) : onlyOne ? (
            <p className="text-[0.6875rem] text-secondary">
              {t('calendar.cannotDeleteLast', {
                defaultValue: 'An account keeps at least one calendar.',
              })}
            </p>
          ) : confirming ? (
            <div className="flex flex-col gap-2.5">
              {/* Said plainly: this is the destructive operation on this
                  screen, and its cost is the events, not the calendar. */}
              <p className="text-[0.6875rem] text-primary">
                {t('calendar.deleteWarning', {
                  defaultValue:
                    'Remove "{name}" and every event on it? They go to Deleted Items on the server.',
                  name: calendar.name,
                })}
              </p>
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() => void run(deleteCalendar(calendar.accountId, calendar.id))}
                  className="inline-flex items-center gap-1.5 rounded-xl bg-rose-500 px-3 py-2 text-xs font-semibold text-white transition-opacity hover:opacity-90 cursor-pointer"
                >
                  <Trash2 size={13} />
                  {t('calendar.deleteConfirm', { defaultValue: 'Remove it' })}
                </button>
                <button
                  type="button"
                  onClick={() => setConfirming(false)}
                  className="rounded-xl px-3 py-2 text-xs font-medium text-secondary transition-colors hover:bg-hover hover:text-primary cursor-pointer"
                >
                  {t('calendar.cancel', { defaultValue: 'Cancel' })}
                </button>
              </div>
            </div>
          ) : (
            <button
              type="button"
              onClick={() => setConfirming(true)}
              className="inline-flex items-center gap-1.5 rounded-xl px-3 py-2 text-xs font-medium text-rose-500 transition-colors hover:bg-rose-500/10 cursor-pointer"
            >
              <Trash2 size={13} />
              {t('calendar.deleteCalendar', { defaultValue: 'Remove calendar' })}
            </button>
          )}
        </div>
      </SettingsGroup>

      {error && <p className="px-1 text-[0.6875rem] text-rose-500">{error}</p>}
    </div>
  )
}
