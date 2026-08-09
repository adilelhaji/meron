// Typography settings: which family the interface uses, which family message
// bodies use, and how large text is drawn overall.
//
// A stored font choice is one string: '' for the default, a `FONT_OPTIONS` id
// for one of the built-in stacks, or any other value — a family name the user
// typed, matched against the fonts installed on their machine.

export type FontOption = {
  id: string
  /** Literal family name, or an i18n key when the option isn't a proper noun. */
  label?: string
  labelKey?: string
  /** CSS family list, without the shared generic/CJK fallbacks. */
  stack: string
}

export const FONT_OPTIONS: FontOption[] = [
  {
    id: 'system',
    labelKey: 'settings.appearance.fontSystem',
    stack: "ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI'",
  },
  { id: 'helvetica', label: 'Helvetica', stack: "'Helvetica Neue', Helvetica, Arial" },
  { id: 'georgia', label: 'Georgia', stack: "Georgia, 'Times New Roman'" },
  {
    id: 'mono',
    labelKey: 'settings.appearance.fontMono',
    stack: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono'",
  },
]

/** Family list appended to a message font inside the sandboxed body iframes,
 *  which can't reach the app's CSS vars. */
const FRAME_FALLBACK = "ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif"

export const DEFAULT_FONT_SCALE = 100
export const MIN_FONT_SCALE = 80
export const MAX_FONT_SCALE = 150

/** Root font size the rem-based text utilities are sized against at 100%. */
export const BASE_ROOT_FONT_SIZE = 16

/** Base sizes the message surfaces are drawn at before any scaling. */
export const BUBBLE_HTML_BASE_PX = 14
export const BUBBLE_CODE_BASE_PX = 12.5
export const READER_HTML_BASE_PX = 16
export const READER_CODE_BASE_PX = 13

/** Whether a stored choice names one of the built-in stacks. */
export function isBuiltinFont(value: string): boolean {
  return FONT_OPTIONS.some((option) => option.id === value)
}

/**
 * The CSS family list for a stored choice, or null for the default. A typed
 * family name is quoted, so "Fira Sans" needs no escaping from the caller.
 */
export function fontStack(value: string): string | null {
  const cleaned = sanitizeFontChoice(value)
  if (!cleaned) return null
  const builtin = FONT_OPTIONS.find((option) => option.id === cleaned)
  if (builtin) return builtin.stack
  return `'${cleaned}'`
}

/**
 * The family message bodies render in, ready for an iframe stylesheet. Falls
 * back to the interface font, then to null when neither is customized (the
 * frames keep their own default stack then).
 */
export function messageFontStack(messageValue: string, uiValue: string): string | null {
  const stack = fontStack(messageValue) ?? fontStack(uiValue)
  return stack ? `${stack}, ${FRAME_FALLBACK}` : null
}

/**
 * Size a message body inside a sandboxed frame, which inherits neither the
 * app's root font size nor the message text scale. Both percentages apply: the
 * app-wide size scales everything, the message size adjusts bodies on top.
 */
export function messageFontSizePx(basePx: number, fontScale: number, messageFontScale: number): number {
  const px = basePx * (clampFontScale(fontScale) / 100) * (clampFontScale(messageFontScale) / 100)
  return Math.round(px * 100) / 100
}

/** Typography baked into a message frame's stylesheet. */
export type MessageFrameFont = {
  /** Family list, or null to keep the frame's own default stack. */
  family: string | null
  bodyPx: number
  codePx: number
}

export type FontPreferences = {
  fontFamily: string
  messageFontFamily: string
  fontScale: number
  messageFontScale: number
}

export function messageFrameFont(prefs: FontPreferences, bodyBasePx: number, codeBasePx: number): MessageFrameFont {
  return {
    family: messageFontStack(prefs.messageFontFamily, prefs.fontFamily),
    bodyPx: messageFontSizePx(bodyBasePx, prefs.fontScale, prefs.messageFontScale),
    codePx: messageFontSizePx(codeBasePx, prefs.fontScale, prefs.messageFontScale),
  }
}

/**
 * Coerce a stored or typed font choice. Family names are stripped of the
 * characters that would let one break out of the quoted CSS value, and capped
 * so a paste can't grow the stylesheet unbounded. Returns null for non-strings
 * so hydration leaves the current value alone.
 */
export function sanitizeFontChoice(raw: unknown): string | null {
  if (typeof raw !== 'string') return null
  return raw
    .replace(/["'`;,{}()<>\\]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 64)
    .trim()
}

export function clampFontScale(scale: number): number {
  if (!Number.isFinite(scale)) return DEFAULT_FONT_SCALE
  return Math.round(Math.min(MAX_FONT_SCALE, Math.max(MIN_FONT_SCALE, scale)))
}

export function sanitizeFontScale(raw: unknown): number | null {
  if (typeof raw !== 'number' || !Number.isFinite(raw)) return null
  return clampFontScale(raw)
}
