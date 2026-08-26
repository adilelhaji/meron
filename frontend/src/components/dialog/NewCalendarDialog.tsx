import { useState } from 'react'
import { useValue } from '@legendapp/state/react'
import { CalendarDays, Cloud, HardDrive, Link2, X } from 'lucide-react'
import { useTranslation } from '../../lib/i18n'
import {
  createCalendar,
  createLocalCalendar,
  subscribeCalendar,
  type CalendarKind,
} from '../../states/calendar'
import { accounts$ } from '../../states/accounts'
import { isRssAccount } from '../../lib/threadActions'

/// Creating a calendar, asking first where it should live.
///
/// The order of the question follows what established calendar apps do: where
/// a calendar lives decides how it syncs and whether it can be edited, so it
/// is asked before anything else rather than inferred from which fields got
/// filled in.
export function NewCalendarDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation()
  const accounts = useValue(accounts$).filter((account) => !isRssAccount(account, account.id))
  const [kind, setKind] = useState<CalendarKind>('account')
  const [accountId, setAccountId] = useState(accounts[0]?.id ?? '')
  const [name, setName] = useState('')
  const [url, setUrl] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const invalid =
    !name.trim() || !accountId || (kind === 'subscribed' && !/^https?:\/\//.test(url.trim()))

  const submit = async () => {
    setBusy(true)
    setError('')
    try {
      if (kind === 'account') await createCalendar(accountId, name.trim())
      else if (kind === 'local') await createLocalCalendar(accountId, name.trim())
      else await subscribeCalendar(accountId, name.trim(), url.trim())
      onClose()
    } catch (err) {
      // Kept open with the message: the URL is the likeliest thing to be
      // wrong, and closing would lose it.
      setError(String(err))
      setBusy(false)
    }
  }

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4" onClick={onClose}>
      <div
        className="w-full max-w-md rounded-2xl border border-border bg-app p-5 shadow-xl"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="flex items-center gap-2 text-sm font-semibold text-primary">
            <CalendarDays size={15} className="text-accent" />
            {t('calendar.addCalendar', { defaultValue: 'Add calendar' })}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="flex h-7 w-7 items-center justify-center rounded-lg text-secondary hover:bg-hover hover:text-primary cursor-pointer"
            aria-label={t('calendar.close', { defaultValue: 'Close' })}
          >
            <X size={15} />
          </button>
        </div>

        <div className="mb-4 grid grid-cols-3 gap-1 rounded-2xl border border-border/80 bg-raised p-1">
          <KindTab
            active={kind === 'account'}
            icon={<Cloud size={16} />}
            label={t('calendar.kindAccount', { defaultValue: 'In an account' })}
            onClick={() => setKind('account')}
          />
          <KindTab
            active={kind === 'local'}
            icon={<HardDrive size={16} />}
            label={t('calendar.kindLocal', { defaultValue: 'On this computer' })}
            onClick={() => setKind('local')}
          />
          <KindTab
            active={kind === 'subscribed'}
            icon={<Link2 size={16} />}
            label={t('calendar.kindSubscribed', { defaultValue: 'From a link' })}
            onClick={() => setKind('subscribed')}
          />
        </div>

        <p className="mb-3 px-0.5 text-[0.6875rem] text-secondary">
          {kind === 'account'
            ? t('calendar.kindAccountHint', {
                defaultValue: 'Created on the account’s server, and available wherever you read that account.',
              })
            : kind === 'local'
              ? t('calendar.kindLocalHint', {
                  defaultValue:
                    'Kept only in this copy of Meron. Nothing else has a copy, so it is lost if this profile is.',
                })
              : t('calendar.kindSubscribedHint', {
                  defaultValue:
                    'Follows a published calendar file. Read-only — it belongs to whoever publishes it.',
                })}
        </p>

        <div className="flex flex-col gap-3">
          <Labelled label={t('calendar.name', { defaultValue: 'Name' })}>
            <input value={name} onChange={(e) => setName(e.target.value)} autoFocus className={inputClass} />
          </Labelled>

          {kind === 'subscribed' && (
            <Labelled label={t('calendar.url', { defaultValue: 'Address' })}>
              <input
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://example.org/calendar.ics"
                className={inputClass}
              />
            </Labelled>
          )}

          {accounts.length > 1 && (
            <Labelled
              label={
                kind === 'account'
                  ? t('calendar.inAccount', { defaultValue: 'Account' })
                  : t('calendar.listedUnder', { defaultValue: 'Listed under' })
              }
            >
              <select
                value={accountId}
                onChange={(e) => setAccountId(e.target.value)}
                className={inputClass}
              >
                {accounts.map((account) => (
                  <option key={account.id} value={account.id}>
                    {account.email}
                  </option>
                ))}
              </select>
            </Labelled>
          )}

          {error && <p className="text-[0.6875rem] text-rose-500">{error}</p>}
        </div>

        <div className="mt-5 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-xl px-3 py-2 text-xs font-medium text-secondary transition-colors hover:bg-hover hover:text-primary cursor-pointer"
          >
            {t('calendar.cancel', { defaultValue: 'Cancel' })}
          </button>
          <button
            type="button"
            disabled={invalid || busy}
            onClick={() => void submit()}
            className="rounded-xl bg-accent px-4 py-2 text-xs font-semibold text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50 cursor-pointer"
          >
            {t('calendar.add', { defaultValue: 'Add' })}
          </button>
        </div>
      </div>
    </div>
  )
}

function KindTab({
  active,
  icon,
  label,
  onClick,
}: {
  active: boolean
  icon: React.ReactNode
  label: string
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex min-w-0 flex-col items-center gap-1 rounded-xl px-2 py-2.5 text-center transition-all cursor-pointer ${
        active
          ? 'bg-chats text-primary shadow-sm ring-1 ring-border/80'
          : 'text-secondary hover:bg-chats/60 hover:text-primary'
      }`}
    >
      <span className={active ? 'text-accent' : ''}>{icon}</span>
      <span className="text-[0.625rem] font-semibold leading-tight">{label}</span>
    </button>
  )
}

const inputClass =
  'w-full rounded-xl border border-border bg-raised px-3 py-2 text-xs text-primary outline-none transition-all focus:border-transparent focus:bg-chats focus:ring-1 focus:ring-accent'

function Labelled({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex w-full flex-col gap-1.5">
      <span className="pl-0.5 text-[0.6875rem] font-semibold text-secondary">{label}</span>
      {children}
    </label>
  )
}
