import { SquarePen, Trash2 } from 'lucide-react'
import { useValue } from '@legendapp/state/react'
import { useTranslation } from '../../lib/i18n'
import {
  calendar$,
  deleteEvent,
  editEvent,
  openEvent,
  type CalendarEvent,
} from '../../states/calendar'
import { FloatingContextMenu } from '../menu/FloatingContextMenu'
import { MenuItem } from '../menu/MenuItem'

export type EventContextMenuState = {
  x: number
  y: number
  event: CalendarEvent
}

/// Right-click menu for an event: the same two actions the editor offers,
/// reachable without opening it. An event on a read-only calendar (a
/// subscription) offers nothing actionable — it belongs to whoever publishes
/// the calendar — so its entries are shown disabled rather than hidden, which
/// says why nothing can be done instead of looking broken.
export function EventContextMenu({
  state,
  onClose,
}: {
  state: EventContextMenuState
  onClose: () => void
}) {
  const { t } = useTranslation()
  const calendars = useValue(calendar$.calendars)
  const readOnly = calendars.some(
    (calendar) =>
      calendar.accountId === state.event.accountId &&
      calendar.id === state.event.calendar_id &&
      calendar.read_only,
  )
  return (
    <FloatingContextMenu
      x={state.x}
      y={state.y}
      onClose={onClose}
      overlay
      overlayClassName="fixed inset-0 z-[60]"
      className="fixed z-[61] min-w-[160px] rounded-xl border border-border bg-header p-1 shadow-xl"
    >
      <MenuItem
        icon={<SquarePen size={13} className="text-accent" />}
        label={t('calendar.editEvent', { defaultValue: 'Edit event' })}
        disabled={readOnly}
        onClick={() => {
          editEvent(state.event)
          onClose()
        }}
      />
      <MenuItem
        icon={<Trash2 size={13} />}
        label={t('calendar.delete', { defaultValue: 'Delete' })}
        danger
        disabled={readOnly}
        onClick={() => {
          // One of a series: the choice between this day and all of them
          // cannot be made here, so it is put where it can be asked rather
          // than answered on the reader's behalf.
          if (state.event.is_recurring && state.event.series_id) openEvent(state.event)
          else void deleteEvent(state.event)
          onClose()
        }}
      />
    </FloatingContextMenu>
  )
}
