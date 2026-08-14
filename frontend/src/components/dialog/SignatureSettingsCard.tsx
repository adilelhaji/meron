import { useEffect, useRef, useState } from 'react'
import { PenLine } from 'lucide-react'
import { EditorContent, useEditor } from '@tiptap/react'
import { StarterKit } from '@tiptap/starter-kit'
import { useValue } from '@legendapp/state/react'
import { useTranslation } from '../../lib/i18n'
import { settings$ } from '../../states/settings'
import { setAccountSignature } from '../../states/accounts'
import { isBlankSignature } from '../../lib/signature'
import type { Account, AccountSignature } from '../../types'
import { ComposerToolbar } from '../composer/ComposerToolbar'
import { SelectRow, SettingsGroup } from './AccountSettingsRows'

// Keystrokes shouldn't each cost a DB write (app-wide) or a bridge round trip
// (per account), so edits settle before they persist.
const SAVE_DEBOUNCE_MS = 600

/**
 * The rich-text editor behind both signature cards. Seeds from `value` when the
 * subject changes (a different account, or General) but never mid-edit, so a
 * save echoing back through state can't yank the caret.
 */
function SignatureEditor({
  seedKey,
  value,
  onChange,
}: {
  seedKey: string
  value: string
  onChange: (html: string) => void
}) {
  const { t } = useTranslation()
  const spellCheck = useValue(settings$.spellCheck)
  const saveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const onChangeRef = useRef(onChange)
  onChangeRef.current = onChange

  const editor = useEditor({
    extensions: [StarterKit.configure({ link: { openOnClick: false } })],
    content: value,
    editorProps: {
      attributes: {
        class: 'tiptap-body focus:outline-none min-h-[110px] px-3.5 py-2.5 text-[0.8125rem] leading-relaxed',
        spellcheck: String(spellCheck),
      },
    },
    onUpdate: ({ editor }) => {
      clearTimeout(saveTimer.current)
      const html = editor.getHTML()
      saveTimer.current = setTimeout(() => onChangeRef.current(isBlankSignature(html) ? '' : html), SAVE_DEBOUNCE_MS)
    },
  })

  // Flush a pending edit rather than dropping it when the card unmounts (the
  // settings dialog closing, or switching to another account).
  useEffect(() => {
    return () => {
      if (!saveTimer.current) return
      clearTimeout(saveTimer.current)
      const html = editor?.getHTML() ?? ''
      onChangeRef.current(isBlankSignature(html) ? '' : html)
    }
  }, [editor])

  useEffect(() => {
    if (!editor) return
    clearTimeout(saveTimer.current)
    saveTimer.current = undefined
    editor.commands.setContent(value || '<p></p>')
  }, [editor, seedKey])

  useEffect(() => {
    editor?.view.dom.setAttribute('spellcheck', String(spellCheck))
  }, [editor, spellCheck])

  const setLink = () => {
    if (!editor) return
    const prev = editor.getAttributes('link').href as string | undefined
    const url = window.prompt('Link URL', prev ?? 'https://')
    if (url === null) return
    if (url === '') {
      editor.chain().focus().extendMarkRange('link').unsetLink().run()
      return
    }
    editor.chain().focus().extendMarkRange('link').setLink({ href: url }).run()
  }

  if (!editor) return null

  return (
    <div>
      <ComposerToolbar editor={editor} onSetLink={setLink} />
      <EditorContent editor={editor} aria-label={t('settings.signature.label')} />
    </div>
  )
}

/**
 * The app-wide signature. Inserted into new messages, replies and forwards for
 * every account that doesn't override it.
 */
export function SignatureSettingsSection() {
  const { t } = useTranslation()
  const signature = useValue(settings$.signature)

  return (
    <SettingsGroup title={t('settings.sections.signature')}>
      <SignatureEditor seedKey="general" value={signature} onChange={(html) => settings$.signature.set(html)} />
      <p className="px-3.5 py-2 text-[0.6875rem] text-secondary">{t('settings.signature.hint')}</p>
    </SettingsGroup>
  )
}

/**
 * Per-account override: follow the app-wide signature, send none, or write one
 * just for this account. The custom text is kept when the mode changes, so
 * flipping away and back doesn't lose it.
 */
export function AccountSignatureCard({ account }: { account: Account }) {
  const { t } = useTranslation()
  const [mode, setMode] = useState<AccountSignature['mode']>('global')
  const [html, setHtml] = useState('')

  // Seed when switching accounts only, so a debounced edit isn't overwritten by
  // the account list refreshing after its own save.
  useEffect(() => {
    setMode(account.signature?.mode ?? 'global')
    setHtml(account.signature?.html ?? '')
  }, [account.id])

  const save = (nextMode: AccountSignature['mode'], nextHtml: string) => {
    setMode(nextMode)
    setHtml(nextHtml)
    void setAccountSignature(account.id, nextMode === 'global' && !nextHtml ? null : { mode: nextMode, html: nextHtml })
  }

  return (
    <SettingsGroup title={t('settings.sections.signature')}>
      <SelectRow
        icon={<PenLine size={15} />}
        title={t('settings.signature.label')}
        hint={t('settings.signature.accountHint')}
        value={mode}
        options={[
          { value: 'global', label: t('settings.signature.modeGlobal') },
          { value: 'none', label: t('settings.signature.modeNone') },
          { value: 'custom', label: t('settings.signature.modeCustom') },
        ]}
        onChange={(next) => save(next as AccountSignature['mode'], html)}
      />
      {mode === 'custom' && (
        <SignatureEditor seedKey={account.id} value={html} onChange={(next) => save('custom', next)} />
      )}
    </SettingsGroup>
  )
}
