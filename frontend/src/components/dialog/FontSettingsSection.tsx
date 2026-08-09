import { useState, type ReactNode } from 'react'
import { ALargeSmall, CaseSensitive, MessagesSquare } from 'lucide-react'
import { useValue } from '@legendapp/state/react'
import { useTranslation } from '../../lib/i18n'
import {
  FONT_OPTIONS,
  MAX_FONT_SCALE,
  MIN_FONT_SCALE,
  clampFontScale,
  isBuiltinFont,
  sanitizeFontChoice,
} from '../../lib/fonts'
import { settings$ } from '../../states/settings'
import { NumberRow, SelectRow, TextRow } from './AccountSettingsRows'

// The typography rows of Settings -> General -> Appearance: the interface font
// and message font (each a preset, or any family installed on the machine), plus
// the app-wide and message text sizes.

const CUSTOM_VALUE = '__custom__'

/** Whether a stored choice is a typed family name rather than a preset. */
function isCustomFont(value: string): boolean {
  return value !== '' && !isBuiltinFont(value)
}

/**
 * A family name the user types. The field keeps the raw text — sanitizing on
 * every keystroke would eat the space in "Fira Sans" before the second word is
 * typed — while the setting stores the sanitized form, so the preview is live.
 */
function CustomFontRow({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState(value)

  return (
    <TextRow
      title={t('settings.appearance.customFont')}
      hint={t('settings.appearance.customFontHint')}
      value={draft}
      placeholder={t('settings.appearance.customFontPlaceholder')}
      onChange={(next) => {
        setDraft(next)
        onChange(sanitizeFontChoice(next) ?? '')
      }}
    />
  )
}

/**
 * A percentage row that holds what's in the field while it's focused, so
 * clearing it to retype doesn't snap the value to the minimum mid-edit. Only
 * an in-range number is committed; leaving the field restores the stored one.
 */
function ScaleRow({
  icon,
  title,
  hint,
  value,
  onChange,
}: {
  icon: ReactNode
  title: string
  hint: string
  value: number
  onChange: (value: number) => void
}) {
  const [draft, setDraft] = useState<string | null>(null)

  return (
    <NumberRow
      icon={icon}
      title={title}
      hint={hint}
      value={draft ?? String(value)}
      min={MIN_FONT_SCALE}
      max={MAX_FONT_SCALE}
      step={5}
      suffix="%"
      onChange={(next) => {
        setDraft(next)
        const scale = Number(next)
        if (next.trim() === '' || !Number.isFinite(scale)) return
        if (scale < MIN_FONT_SCALE || scale > MAX_FONT_SCALE) return
        onChange(clampFontScale(scale))
      }}
      onBlur={() => setDraft(null)}
    />
  )
}

export function FontSettingsSection() {
  const { t } = useTranslation()
  const fontFamily = useValue(settings$.fontFamily)
  const messageFontFamily = useValue(settings$.messageFontFamily)
  const fontScale = useValue(settings$.fontScale)
  const messageFontScale = useValue(settings$.messageFontScale)

  // "Custom…" stays selected while the field is still empty, which no stored
  // value can express (an empty choice means the default).
  const [uiCustom, setUiCustom] = useState(() => isCustomFont(fontFamily))
  const [messageCustom, setMessageCustom] = useState(() => isCustomFont(messageFontFamily))

  const presetOptions = FONT_OPTIONS.map((option) => ({
    value: option.id,
    label: option.labelKey ? t(option.labelKey) : option.label!,
  }))

  const familyOptions = (defaultLabel: string) => [
    { value: '', label: defaultLabel },
    ...presetOptions,
    { value: CUSTOM_VALUE, label: t('settings.appearance.fontCustom') },
  ]

  const selectFamily = (value: string, setValue: (next: string) => void, setCustom: (custom: boolean) => void) => {
    if (value === CUSTOM_VALUE) {
      setCustom(true)
      setValue('')
      return
    }
    setCustom(false)
    setValue(value)
  }

  return (
    <>
      <SelectRow
        icon={<CaseSensitive size={15} />}
        title={t('settings.appearance.uiFont')}
        hint={t('settings.appearance.uiFontHint')}
        value={uiCustom ? CUSTOM_VALUE : fontFamily}
        options={familyOptions(t('settings.appearance.fontDefault'))}
        onChange={(value) => selectFamily(value, (next) => settings$.fontFamily.set(next), setUiCustom)}
      />
      {uiCustom && <CustomFontRow value={fontFamily} onChange={(value) => settings$.fontFamily.set(value)} />}
      <ScaleRow
        icon={<ALargeSmall size={15} />}
        title={t('settings.appearance.textSize')}
        hint={t('settings.appearance.textSizeHint')}
        value={fontScale}
        onChange={(value) => settings$.fontScale.set(value)}
      />
      <SelectRow
        icon={<MessagesSquare size={15} />}
        title={t('settings.appearance.messageFont')}
        hint={t('settings.appearance.messageFontHint')}
        value={messageCustom ? CUSTOM_VALUE : messageFontFamily}
        options={familyOptions(t('settings.appearance.fontSameAsUi'))}
        onChange={(value) => selectFamily(value, (next) => settings$.messageFontFamily.set(next), setMessageCustom)}
      />
      {messageCustom && (
        <CustomFontRow value={messageFontFamily} onChange={(value) => settings$.messageFontFamily.set(value)} />
      )}
      <ScaleRow
        icon={<ALargeSmall size={15} />}
        title={t('settings.appearance.messageTextSize')}
        hint={t('settings.appearance.messageTextSizeHint')}
        value={messageFontScale}
        onChange={(value) => settings$.messageFontScale.set(value)}
      />
    </>
  )
}
