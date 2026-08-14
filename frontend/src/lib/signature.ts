import type { Account, AccountSignature } from '../types'

// Signatures are stored as HTML: one app-wide signature in settings, plus an
// optional per-account override (see states/settings.ts and types.ts). They are
// inserted into the draft body when a composer opens — the user can edit or
// delete the text like any other content — rather than being stapled on at send
// time, so what the composer shows is what goes out.

/** Whether signature HTML carries any visible content (text or an image). */
export function isBlankSignature(html: string): boolean {
  if (!html.trim()) return true
  const stripped = html
    .replace(/<(img|hr|br)\b[^>]*>/gi, 'x')
    .replace(/<[^>]*>/g, '')
    .replace(/&nbsp;/gi, ' ')
  return !stripped.trim()
}

/**
 * The signature HTML an account actually sends: its own override, nothing when
 * it opts out, or the app-wide signature. Accounts that can't send (RSS) and
 * blank signatures resolve to ''.
 */
export function resolveSignature(account: Account | undefined | null, globalHtml: string): string {
  const override: AccountSignature | null | undefined = account?.signature
  const html = override?.mode === 'none' ? '' : override?.mode === 'custom' ? override.html : globalHtml
  return isBlankSignature(html) ? '' : html
}

export type ComposeBody = { rich: boolean; html: string; text: string }

/**
 * A resolved signature in both of the forms a draft body can need. The caller
 * derives `text` (only plaintext drafts need it), which keeps this module free
 * of the DOM that HTML-to-text conversion requires.
 */
export type Signature = { html: string; text: string }

/**
 * Where the signature lands relative to whatever the draft was seeded with.
 * 'aboveQuote' for a forward, whose seeded body is the quoted message: the
 * signature belongs between what the user is about to type and the quote, as
 * in Gmail and Apple Mail. 'belowText' for everything else, where the seed is
 * the user's own text (a quick reply carried into the full editor) or nothing.
 */
export type SignaturePlacement = 'aboveQuote' | 'belowText'

/** Place a signature in a draft body, leaving a blank line for the cursor. */
export function bodyWithSignature(
  body: ComposeBody,
  signature: Signature,
  placement: SignaturePlacement = 'belowText',
): ComposeBody {
  if (!signature.html) return body
  if (body.rich) {
    const html =
      placement === 'aboveQuote' ? `<p></p>${signature.html}${body.html}` : `${body.html}<p></p>${signature.html}`
    return { ...body, html }
  }
  const signatureText = signature.text
  if (!signatureText) return body
  const seeded = body.text.trim()
  if (!seeded) return { ...body, text: `\n\n${signatureText}` }
  const text = placement === 'aboveQuote' ? `\n\n${signatureText}\n\n${seeded}` : `${seeded}\n\n${signatureText}`
  return { ...body, text }
}
