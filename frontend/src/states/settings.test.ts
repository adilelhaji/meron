import { afterEach, describe, expect, it } from 'bun:test'
import { EMPTY_PROXY, hydrateSettings, isProxyUsable, sanitizeKanbanBoards, sanitizeProxy, settings$ } from './settings'

const baseBoard = {
  id: 'kb-1',
  name: 'Work',
  columns: [{ accountId: 'acc-1', folderId: 'inbox' }],
}

describe('sanitizeKanbanBoards', () => {
  it('keeps boards without customization fields untouched', () => {
    expect(sanitizeKanbanBoards([baseBoard])).toEqual([baseBoard])
  })

  it('accepts an app-managed avatar url', () => {
    const boards = sanitizeKanbanBoards([{ ...baseBoard, avatarUrl: '/media/avatars/kb-1/a.png' }])
    expect(boards?.[0].avatarUrl).toBe('/media/avatars/kb-1/a.png')
  })

  it('drops avatar urls outside /media/avatars/', () => {
    for (const avatarUrl of ['https://example.com/a.png', '/media/wallpapers/kb-1/a.png', 7, '']) {
      const boards = sanitizeKanbanBoards([{ ...baseBoard, avatarUrl }])
      expect(boards?.[0].avatarUrl).toBeUndefined()
    }
  })

  it('accepts preset and custom wallpapers', () => {
    expect(
      sanitizeKanbanBoards([{ ...baseBoard, wallpaper: { kind: 'preset', presetId: 'dots' } }])?.[0].wallpaper,
    ).toEqual({ kind: 'preset', presetId: 'dots' })
    expect(
      sanitizeKanbanBoards([{ ...baseBoard, wallpaper: { kind: 'custom', url: '/media/wallpapers/kb-1/w.png' } }])?.[0]
        .wallpaper,
    ).toEqual({ kind: 'custom', url: '/media/wallpapers/kb-1/w.png' })
  })

  it('drops invalid wallpapers', () => {
    for (const wallpaper of [
      { kind: 'preset', presetId: 'nope' },
      { kind: 'custom', url: 'https://example.com/w.png' },
      'dots',
      null,
    ]) {
      const boards = sanitizeKanbanBoards([{ ...baseBoard, wallpaper }])
      expect(boards?.[0].wallpaper).toBeUndefined()
    }
  })
})

describe('spellCheck setting', () => {
  afterEach(() => {
    settings$.spellCheck.set(true)
  })

  it('defaults spell check on', () => {
    expect(settings$.spellCheck.get()).toBe(true)
  })

  it('hydrates a persisted spell check preference', () => {
    hydrateSettings({ spell_check: false })
    expect(settings$.spellCheck.get()).toBe(false)

    hydrateSettings({ spell_check: true })
    expect(settings$.spellCheck.get()).toBe(true)
  })

  it('ignores invalid persisted spell check values', () => {
    settings$.spellCheck.set(false)
    hydrateSettings({ spell_check: 'true' })
    expect(settings$.spellCheck.get()).toBe(false)
  })
})

describe('proxy setting', () => {
  afterEach(() => {
    settings$.proxy.set(EMPTY_PROXY)
  })

  it('defaults to no proxy', () => {
    expect(settings$.proxy.get()).toEqual(EMPTY_PROXY)
  })

  it('hydrates a persisted proxy', () => {
    hydrateSettings({ proxy: { mode: 'socks5', host: ' 127.0.0.1 ', port: 1080, username: 'u', password: 'p' } })
    expect(settings$.proxy.get()).toEqual({
      mode: 'socks5',
      host: '127.0.0.1',
      port: 1080,
      username: 'u',
      password: 'p',
    })
  })

  it('rejects unknown modes and out-of-range ports', () => {
    expect(sanitizeProxy({ mode: 'ftp', host: 'h', port: 1 })).toBeNull()
    expect(sanitizeProxy('socks5')).toBeNull()
    expect(sanitizeProxy({ mode: 'http', host: 'h', port: 70000 })?.port).toBe(0)
    expect(sanitizeProxy({ mode: 'http', host: 'h', port: -1 })?.port).toBe(0)
  })

  it('treats a half-filled proxy as unusable', () => {
    expect(isProxyUsable({ mode: 'off', host: 'h', port: 1080, username: '', password: '' })).toBe(false)
    expect(isProxyUsable({ mode: 'http', host: '', port: 8080, username: '', password: '' })).toBe(false)
    expect(isProxyUsable({ mode: 'http', host: 'h', port: 0, username: '', password: '' })).toBe(false)
    expect(isProxyUsable({ mode: 'http', host: 'h', port: 8080, username: '', password: '' })).toBe(true)
  })
})
