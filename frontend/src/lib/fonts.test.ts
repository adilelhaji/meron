import { describe, expect, it } from 'bun:test'
import {
  BUBBLE_HTML_BASE_PX,
  DEFAULT_FONT_SCALE,
  FONT_OPTIONS,
  MAX_FONT_SCALE,
  MIN_FONT_SCALE,
  clampFontScale,
  fontStack,
  isBuiltinFont,
  messageFontSizePx,
  messageFontStack,
  messageFrameFont,
  sanitizeFontChoice,
  sanitizeFontScale,
} from './fonts'

describe('fontStack', () => {
  it('leaves the default unset', () => {
    expect(fontStack('')).toBeNull()
    expect(fontStack('   ')).toBeNull()
  })

  it('resolves a built-in option to its stack', () => {
    const mono = FONT_OPTIONS.find((option) => option.id === 'mono')!
    expect(fontStack('mono')).toBe(mono.stack)
    expect(isBuiltinFont('mono')).toBe(true)
  })

  it('quotes a typed family name', () => {
    expect(fontStack('Fira Sans')).toBe("'Fira Sans'")
    expect(isBuiltinFont('Fira Sans')).toBe(false)
  })

  it('cannot break out of the quoted value', () => {
    expect(fontStack("Evil'; color: red; font-family: 'x")).toBe("'Evil color: red font-family: x'")
  })
})

describe('sanitizeFontChoice', () => {
  it('rejects non-strings so hydration keeps the current value', () => {
    expect(sanitizeFontChoice(42)).toBeNull()
    expect(sanitizeFontChoice(undefined)).toBeNull()
  })

  it('collapses whitespace and caps the length', () => {
    expect(sanitizeFontChoice('  Fira   Sans  ')).toBe('Fira Sans')
    expect(sanitizeFontChoice('a'.repeat(200))).toHaveLength(64)
  })
})

describe('font scale', () => {
  it('clamps to the supported range', () => {
    expect(clampFontScale(10)).toBe(MIN_FONT_SCALE)
    expect(clampFontScale(999)).toBe(MAX_FONT_SCALE)
    expect(clampFontScale(112.4)).toBe(112)
    expect(clampFontScale(Number.NaN)).toBe(DEFAULT_FONT_SCALE)
  })

  it('only accepts stored numbers', () => {
    expect(sanitizeFontScale('120')).toBeNull()
    expect(sanitizeFontScale(120)).toBe(120)
  })
})

describe('message typography', () => {
  it('falls back to the interface font, then to the frame default', () => {
    expect(messageFontStack('', '')).toBeNull()
    expect(messageFontStack('', 'Fira Sans')).toStartWith("'Fira Sans', ")
    expect(messageFontStack('georgia', 'Fira Sans')).toStartWith("Georgia, 'Times New Roman', ")
  })

  it('multiplies the app size and the message size', () => {
    expect(messageFontSizePx(14, 100, 100)).toBe(14)
    expect(messageFontSizePx(14, 150, 100)).toBe(21)
    expect(messageFontSizePx(14, 100, 150)).toBe(21)
    expect(messageFontSizePx(14, 150, 150)).toBe(31.5)
  })

  it('builds a frame font from the stored preferences', () => {
    const font = messageFrameFont(
      { fontFamily: '', messageFontFamily: 'georgia', fontScale: 100, messageFontScale: 120 },
      BUBBLE_HTML_BASE_PX,
      12.5,
    )
    expect(font.family).toStartWith('Georgia, ')
    expect(font.bodyPx).toBe(16.8)
    expect(font.codePx).toBe(15)
  })
})
