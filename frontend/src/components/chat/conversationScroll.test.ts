import { describe, expect, it } from 'bun:test'
import {
  ANCHOR_GAP_PX,
  anchorScrollTop,
  BOTTOM_STICK_PX,
  isUserScroll,
  resolveOpenScroll,
  resolveResizeScrollTop,
  type ScrollMetrics,
} from './conversationScroll'

const CONTAINER_TOP = 100
const VIEWPORT = 800

function metrics(scrollTop: number, scrollHeight: number): ScrollMetrics {
  return { scrollTop, scrollHeight, clientHeight: VIEWPORT }
}

describe('anchorScrollTop', () => {
  it('puts the target just below the top of the container', () => {
    expect(anchorScrollTop(1000, CONTAINER_TOP)).toBe(1000 - CONTAINER_TOP - ANCHOR_GAP_PX)
  })

  it('never scrolls above the top', () => {
    expect(anchorScrollTop(CONTAINER_TOP, CONTAINER_TOP)).toBe(0)
    expect(anchorScrollTop(0, CONTAINER_TOP)).toBe(0)
  })
})

describe('isUserScroll', () => {
  it('ignores the scroll event our own positioning fires', () => {
    expect(isUserScroll(1276, 1276)).toBe(false)
  })

  it('tolerates the sub-pixel position a browser reports back', () => {
    expect(isUserScroll(1275.5, 1276)).toBe(false)
  })

  it('recognises the reader moving the view', () => {
    // Dragging or clicking the scrollbar dispatches no mouse events to the
    // element, so this position check is what releases the anchor.
    expect(isUserScroll(2400, 1276)).toBe(true)
    expect(isUserScroll(1200, 1276)).toBe(true)
  })

  it("treats a scroll before any positioning as the reader's", () => {
    expect(isUserScroll(40, null)).toBe(true)
  })
})

describe('resolveOpenScroll on a fresh open', () => {
  const open = (over: Partial<Parameters<typeof resolveOpenScroll>[0]> = {}) =>
    resolveOpenScroll({
      isNewThread: true,
      grew: true,
      savedScrollTop: null,
      metrics: metrics(0, 2000),
      containerOffsetTop: CONTAINER_TOP,
      hasUnread: false,
      firstUnreadOffsetTop: null,
      ...over,
    })

  it('lands on the first unread message', () => {
    expect(open({ hasUnread: true, firstUnreadOffsetTop: 1400 })).toEqual({
      scrollTop: 1400 - CONTAINER_TOP - ANCHOR_GAP_PX,
      pin: true,
    })
  })

  it('pins the first unread even when it sits within a screen of the bottom', () => {
    // The regression: with the two unread mails near the end of the thread the
    // anchor is close to the bottom, so an unpinned view got snapped past them
    // by the first body-height resize.
    const plan = open({ hasUnread: true, firstUnreadOffsetTop: 1900, metrics: metrics(0, 2000) })
    expect(plan.pin).toBe(true)
    expect(plan.scrollTop).toBe(1900 - CONTAINER_TOP - ANCHOR_GAP_PX)
  })

  it('lands on the newest message when the thread is fully read', () => {
    expect(open({ metrics: metrics(0, 2000) })).toEqual({ scrollTop: 2000, pin: false })
  })

  it('leaves the view alone while unread messages are still unrendered', () => {
    expect(open({ hasUnread: true, firstUnreadOffsetTop: null })).toEqual({ scrollTop: null, pin: false })
  })
})

describe('resolveOpenScroll on an already-open thread', () => {
  const reopen = (over: Partial<Parameters<typeof resolveOpenScroll>[0]> = {}) =>
    resolveOpenScroll({
      isNewThread: false,
      grew: false,
      savedScrollTop: null,
      metrics: metrics(500, 3000),
      containerOffsetTop: CONTAINER_TOP,
      hasUnread: false,
      firstUnreadOffsetTop: null,
      ...over,
    })

  it('does not move the view when messages merely re-render', () => {
    expect(reopen({ hasUnread: true, firstUnreadOffsetTop: 900 })).toEqual({ scrollTop: null, pin: false })
  })

  it('follows a newly arrived message for a reader at the bottom', () => {
    expect(reopen({ grew: true, metrics: metrics(3000 - VIEWPORT, 3000) })).toEqual({ scrollTop: 3000, pin: false })
  })

  it('leaves a reader who scrolled up where they are when a message arrives', () => {
    expect(reopen({ grew: true, metrics: metrics(3000 - VIEWPORT - BOTTOM_STICK_PX - 1, 3000) })).toEqual({
      scrollTop: null,
      pin: false,
    })
  })

  it('restores the saved position when returning to a thread', () => {
    expect(reopen({ savedScrollTop: 640 })).toEqual({ scrollTop: 640, pin: false })
  })

  it('clamps a saved position that no longer exists to the bottom', () => {
    expect(reopen({ savedScrollTop: 9999, metrics: metrics(0, 3000) })).toEqual({
      scrollTop: 3000 - VIEWPORT,
      pin: false,
    })
  })
})

describe('resolveResizeScrollTop', () => {
  const resize = (over: Partial<Parameters<typeof resolveResizeScrollTop>[0]> = {}) =>
    resolveResizeScrollTop({
      metrics: metrics(0, 4000),
      previousScrollHeight: 4000,
      containerOffsetTop: CONTAINER_TOP,
      pinnedOffsetTop: null,
      ...over,
    })

  it('re-anchors to the pinned message as bodies grow', () => {
    expect(resize({ pinnedOffsetTop: 2600, metrics: metrics(1200, 6000) })).toBe(2600 - CONTAINER_TOP - ANCHOR_GAP_PX)
  })

  it('keeps the pin even when the view sat at the pre-resize bottom', () => {
    // A cold open positions the first unread near the bottom of the still
    // placeholder-sized content; without the pin this resize would snap to the
    // new bottom and hide the unread messages.
    expect(
      resize({
        pinnedOffsetTop: 1900,
        previousScrollHeight: 2000,
        metrics: metrics(2000 - VIEWPORT, 6000),
      }),
    ).toBe(1900 - CONTAINER_TOP - ANCHOR_GAP_PX)
  })

  it('sticks to the bottom for a reader who was already there', () => {
    expect(resize({ previousScrollHeight: 4000, metrics: metrics(4000 - VIEWPORT, 5000) })).toBe(5000)
  })

  it('does not yank back a reader who scrolled up', () => {
    expect(
      resize({ previousScrollHeight: 4000, metrics: metrics(4000 - VIEWPORT - BOTTOM_STICK_PX - 1, 5000) }),
    ).toBeNull()
  })

  it('measures the distance from the bottom before the resize, not after', () => {
    // Content that grew far below the fold must not count as "the reader is
    // near the bottom" just because they were before it appeared.
    expect(resize({ previousScrollHeight: 4000, metrics: metrics(4000 - VIEWPORT, 40000) })).toBe(40000)
    expect(resize({ previousScrollHeight: 40000, metrics: metrics(4000 - VIEWPORT, 40000) })).toBeNull()
  })
})
