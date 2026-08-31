import { describe, expect, it } from 'bun:test'
import { flushQueuedSends, isQueued, queueSend, queuedCount, undoQueuedSend } from './sendQueue'

describe('the grace period before a message goes', () => {
  it('sends when it runs out, and not before', async () => {
    let sent = 0
    queueSend('a', 0.02, () => sent++, () => {})
    expect(isQueued('a')).toBe(true)
    expect(sent).toBe(0)

    await new Promise((resolve) => setTimeout(resolve, 40))
    expect(sent).toBe(1)
    expect(isQueued('a')).toBe(false)
  })

  it('undoes a send that has not gone, and says so', () => {
    let sent = 0
    let undone = 0
    queueSend('b', 10, () => sent++, () => undone++)

    expect(undoQueuedSend('b')).toBe(true)
    expect(undone).toBe(1)
    expect(sent).toBe(0)

    // Already gone: a message cannot be recalled, and saying otherwise would
    // be a lie about something the reader cares a great deal about.
    expect(undoQueuedSend('b')).toBe(false)
    expect(undone).toBe(1)
  })

  it('sends everything still waiting when the app closes', () => {
    const sent: string[] = []
    queueSend('c', 10, () => sent.push('c'), () => {})
    queueSend('d', 10, () => sent.push('d'), () => {})
    expect(queuedCount()).toBe(2)

    flushQueuedSends()

    // Lost is the one outcome the grace period must never produce: it is a
    // chance to take something back, not a chance to lose it.
    expect(sent.sort()).toEqual(['c', 'd'])
    expect(queuedCount()).toBe(0)
  })

  it('does not stack two timers on one message', async () => {
    let sent = 0
    queueSend('e', 10, () => sent++, () => {})
    queueSend('e', 0.02, () => sent++, () => {})
    await new Promise((resolve) => setTimeout(resolve, 40))
    expect(sent).toBe(1)
  })
})
