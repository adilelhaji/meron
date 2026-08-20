import { describe, expect, it } from 'bun:test'
import { prepareBubbleHtml } from './bubbleHtml'

// A newsletter's own reset is `html, body { height: 100% !important }`, so the
// override only wins as an inline declaration — those outrank every stylesheet
// rule of the same importance, wherever the sender's `<style>` happens to sit.
const sizing = (prepared: string) => {
  const doc = new DOMParser().parseFromString(prepared, 'text/html')
  return [doc.documentElement, doc.body].map((el) => el.getAttribute('style') ?? '')
}

describe('prepareBubbleHtml', () => {
  it('lets newsletter documents grow beyond the placeholder frame', () => {
    const html = `
      <html>
        <head>
          <style>html, body { height: 100% !important; }</style>
        </head>
        <body><p>Visible message</p></body>
      </html>
    `

    const prepared = prepareBubbleHtml(html)

    for (const style of sizing(prepared)) {
      expect(style).toContain('height: auto !important')
      expect(style).toContain('min-height: 0 !important')
    }
    expect(prepared).toContain('Visible message')
  })

  it('outranks a reset that the sender put inside the body', () => {
    // ESP templates commonly emit their reset/media-query block after <body>
    // starts; the parser leaves it there, so a head-only override would lose.
    const html = `
      <html>
        <body>
          <style>html, body { height: 100% !important; }</style>
          <p>Visible message</p>
        </body>
      </html>
    `

    const prepared = prepareBubbleHtml(html)

    for (const style of sizing(prepared)) {
      expect(style).toContain('height: auto !important')
    }
    expect(prepared).toContain('Visible message')
  })

  it('leaves the body structure untouched', () => {
    const html = '<html><body><p>First</p><table><tr><td>Last</td></tr></table></body></html>'

    const doc = new DOMParser().parseFromString(prepareBubbleHtml(html), 'text/html')

    expect(doc.body.lastElementChild?.tagName).toBe('TABLE')
    expect(doc.querySelector('body > table:last-child')).not.toBeNull()
  })
})
