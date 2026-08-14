import { describe, expect, it } from 'bun:test'
import type { Account } from '../types'
import { bodyWithSignature, isBlankSignature, resolveSignature } from './signature'

const account = (overrides: Partial<Account> = {}): Account => ({
  id: 'acc',
  email: 'me@example.com',
  display_name: 'Me',
  provider: 'custom',
  auth_type: 'password',
  imap_host: 'imap.example.com',
  imap_port: 993,
  smtp_host: 'smtp.example.com',
  smtp_port: 465,
  tls: true,
  ...overrides,
})

describe('isBlankSignature', () => {
  it('treats empty markup as blank', () => {
    expect(isBlankSignature('')).toBe(true)
    expect(isBlankSignature('<p></p>')).toBe(true)
    expect(isBlankSignature('<p>&nbsp;</p>')).toBe(true)
  })

  it('keeps text and images', () => {
    expect(isBlankSignature('<p>Ping</p>')).toBe(false)
    expect(isBlankSignature('<p><img src="/media/logo.png"></p>')).toBe(false)
  })
})

describe('resolveSignature', () => {
  it('falls back to the app-wide signature when the account has no override', () => {
    expect(resolveSignature(account(), '<p>App</p>')).toBe('<p>App</p>')
  })

  it('honours the account override', () => {
    expect(resolveSignature(account({ signature: { mode: 'custom', html: '<p>Mine</p>' } }), '<p>App</p>')).toBe(
      '<p>Mine</p>',
    )
    expect(resolveSignature(account({ signature: { mode: 'none', html: '<p>Mine</p>' } }), '<p>App</p>')).toBe('')
    expect(resolveSignature(account({ signature: { mode: 'global', html: '<p>Mine</p>' } }), '<p>App</p>')).toBe(
      '<p>App</p>',
    )
  })

  it('resolves a blank signature to nothing', () => {
    expect(resolveSignature(account(), '<p></p>')).toBe('')
    expect(resolveSignature(undefined, '')).toBe('')
  })
})

describe('bodyWithSignature', () => {
  const rich = { rich: true, html: '', text: '' }
  const plain = { rich: false, html: '', text: '' }

  const sig = (html: string, text: string) => ({ html, text })

  it('leaves the body alone without a signature', () => {
    expect(bodyWithSignature(rich, sig('', ''))).toEqual(rich)
  })

  it('appends to a blank rich body with a line to type on', () => {
    expect(bodyWithSignature(rich, sig('<p>Ping</p>', 'Ping')).html).toBe('<p></p><p>Ping</p>')
  })

  it('puts the signature above a forwarded quote', () => {
    const quoted = { rich: true, html: '<blockquote>old</blockquote>', text: '' }
    expect(bodyWithSignature(quoted, sig('<p>Ping</p>', 'Ping'), 'aboveQuote').html).toBe(
      '<p></p><p>Ping</p><blockquote>old</blockquote>',
    )
  })

  it('keeps carried-over text above the signature', () => {
    const typed = { rich: true, html: '<p>typed</p>', text: '' }
    expect(bodyWithSignature(typed, sig('<p>Ping</p>', 'Ping')).html).toBe('<p>typed</p><p></p><p>Ping</p>')
  })

  it('uses the plaintext form for a plaintext draft', () => {
    expect(bodyWithSignature(plain, sig('<p>Ping</p><p>Pong</p>', 'Ping\n\nPong')).text).toBe('\n\nPing\n\nPong')
    expect(bodyWithSignature({ ...plain, text: 'typed' }, sig('<p>Ping</p>', 'Ping')).text).toBe('typed\n\nPing')
    expect(bodyWithSignature({ ...plain, text: '> quoted' }, sig('<p>Ping</p>', 'Ping'), 'aboveQuote').text).toBe(
      '\n\nPing\n\n> quoted',
    )
  })
})
