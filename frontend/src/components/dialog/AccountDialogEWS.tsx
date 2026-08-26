import { useState } from 'react'
import { Eye, EyeOff } from 'lucide-react'
import { useTranslation } from '../../lib/i18n'
import { Field } from '../field/Field'
import type { AccountDialogController } from './useAccountDialog'
import type { DialogClasses } from './accountDialogStyles'

/// Setup panel for an Exchange (EWS) account. Far fewer fields than the IMAP
/// panel: EWS carries mail and submission over one HTTPS endpoint, so there
/// are no host/port pairs and no TLS mode to choose.
export function AccountDialogEWS({
  ctl,
  classes,
}: {
  ctl: AccountDialogController
  classes: DialogClasses
  isSetup: boolean
}) {
  const { t } = useTranslation()
  const { form, setForm, save, saveDisabled, editing } = ctl
  const { inputClass, fieldLabelClass } = classes
  const [showPassword, setShowPassword] = useState(false)

  return (
    <>
      <Field
        label={t('accounts.fields.emailAddress')}
        value={form.email}
        onChange={(email) => setForm((f) => ({ ...f, email }))}
        inputClassName={inputClass}
        labelClassName={fieldLabelClass}
        // The address is the account's primary key in the store; renaming is
        // Remove + Add, deliberately, as on the IMAP panel.
        disabled={editing}
      />
      <Field
        label={t('accounts.fields.ewsUrl', { defaultValue: 'EWS server URL' })}
        value={form.ews_url}
        onChange={(ews_url) => setForm((f) => ({ ...f, ews_url }))}
        placeholder="https://mail.example.org/EWS/Exchange.asmx"
        inputClassName={inputClass}
        labelClassName={fieldLabelClass}
      />
      <Field
        label={t('accounts.fields.username')}
        value={form.username}
        onChange={(username) => setForm((f) => ({ ...f, username }))}
        placeholder={t('accounts.fields.ewsUsernameHint', {
          defaultValue: 'user@example.org or DOMAIN\\user',
        })}
        inputClassName={inputClass}
        labelClassName={fieldLabelClass}
      />
      <Field
        label={t('accounts.fields.senderNameOutgoing')}
        value={form.sender_name}
        onChange={(sender_name) => setForm((f) => ({ ...f, sender_name }))}
        inputClassName={inputClass}
        labelClassName={fieldLabelClass}
      />
      <label className="flex flex-col gap-1.5 w-full">
        <span className={`pl-0.5 ${fieldLabelClass ?? 'text-[0.6875rem] font-semibold text-secondary'}`}>
          {editing ? t('accounts.fields.passwordUnchanged') : t('accounts.fields.password')}
        </span>
        <span className="relative flex items-center">
          <input
            type={showPassword ? 'text' : 'password'}
            value={form.password}
            onChange={(event) => setForm((f) => ({ ...f, password: event.target.value }))}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !saveDisabled) {
                e.preventDefault()
                void save()
              }
            }}
            className={`${inputClass ?? 'w-full text-xs py-2 px-3.5 rounded-xl border border-border bg-raised text-primary placeholder-secondary focus:ring-1 focus:ring-accent focus:border-transparent focus:bg-chats transition-all outline-none'} pr-11`}
          />
          <button
            type="button"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => setShowPassword((value) => !value)}
            className="absolute right-2.5 flex h-7 w-7 items-center justify-center rounded-lg text-secondary transition-colors hover:bg-hover hover:text-primary cursor-pointer"
            aria-label={
              showPassword
                ? t('accounts.actions.hidePassword', { defaultValue: 'Hide password' })
                : t('accounts.actions.showPassword', { defaultValue: 'Show password' })
            }
          >
            {showPassword ? <EyeOff size={14} /> : <Eye size={14} />}
          </button>
        </span>
      </label>
      <p className="text-[0.6875rem] text-secondary px-1 -mt-1">
        {t('accounts.setup.ewsHint', {
          defaultValue:
            'For on-premises Exchange servers that expose Exchange Web Services over HTTPS. Mail and calendar are set up together.',
        })}
      </p>
    </>
  )
}
