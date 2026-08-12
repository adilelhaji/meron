// The scroll-positioning arithmetic behind useConversationScroll, kept free of
// DOM lookups so it can be unit-tested. The hook measures the container and its
// message elements and feeds the numbers in; these functions decide where the
// view should land. Both conversation layouts (chat bubbles and the traditional
// rows) share the hook, so they share these rules.

/** Breathing room left above a message the view anchors to. */
export const ANCHOR_GAP_PX = 24

/** Below this distance from the bottom the view counts as "at the bottom", and
 *  content growing underneath keeps it there. */
export const BOTTOM_STICK_PX = 160

/** How long after positioning the view keeps re-anchoring to its target while
 *  asynchronously measured bodies settle. Mirrors THREAD_OPEN_ANCHOR_WINDOW_MS
 *  on mobile. */
export const OPEN_ANCHOR_WINDOW_MS = 1800

/** Slack for the sub-pixel scroll positions a browser can report back after a
 *  programmatic assignment. */
const SCROLL_MATCH_TOLERANCE_PX = 1

/**
 * Whether a scroll event came from the reader rather than from our own
 * positioning. Scrollbar drags and clicks dispatch no mouse events to the
 * element in Chromium, so the only signal that separates them from the
 * anchoring we do ourselves is the position: anything we did not just set is
 * the reader moving the view. `expectedScrollTop` is null before we have
 * positioned anything, when every scroll is the reader's.
 */
export function isUserScroll(scrollTop: number, expectedScrollTop: number | null): boolean {
  if (expectedScrollTop === null) return true
  return Math.abs(scrollTop - expectedScrollTop) > SCROLL_MATCH_TOLERANCE_PX
}

export type ScrollMetrics = {
  scrollTop: number
  scrollHeight: number
  clientHeight: number
}

/** Where to scroll so `targetOffsetTop` sits just below the container's top. */
export function anchorScrollTop(targetOffsetTop: number, containerOffsetTop: number): number {
  return Math.max(0, targetOffsetTop - containerOffsetTop - ANCHOR_GAP_PX)
}

function bottomScrollTop(metrics: ScrollMetrics): number {
  return Math.max(0, metrics.scrollHeight - metrics.clientHeight)
}

/**
 * Where a content resize should leave the view. `previousScrollHeight` is the
 * height measured before this resize, so the distance-from-bottom test asks
 * "was the reader at the bottom", not "is the new content past the fold".
 * Returns null to leave the scroll position alone.
 */
export function resolveResizeScrollTop({
  metrics,
  previousScrollHeight,
  containerOffsetTop,
  pinnedOffsetTop,
}: {
  metrics: ScrollMetrics
  previousScrollHeight: number
  containerOffsetTop: number
  /** offsetTop of the pinned message, or null when nothing is pinned. */
  pinnedOffsetTop: number | null
}): number | null {
  // A pinned target wins: bodies growing above it must not push it out of view,
  // and its own growth must not read as "the reader is at the bottom".
  if (pinnedOffsetTop !== null) {
    return anchorScrollTop(pinnedOffsetTop, containerOffsetTop)
  }
  // Keep the view pinned to the bottom only when it already was (content grew
  // under the fold, e.g. images loading after open). A reader scrolled up — to
  // star or reread something — must not be yanked back down.
  const previousDistanceFromBottom = previousScrollHeight - metrics.scrollTop - metrics.clientHeight
  if (previousDistanceFromBottom > BOTTOM_STICK_PX) return null
  return metrics.scrollHeight
}

export type OpenScrollPlan = {
  /** Target scroll position, or null to leave the view where it is. */
  scrollTop: number | null
  /** Whether the target should be held against resizes for the anchor window. */
  pin: boolean
}

/**
 * Where the view belongs when a thread renders: a restored position when
 * returning to a thread, the first unread on a fresh open, the newest message
 * when everything is read, and nothing at all when messages merely re-render.
 */
export function resolveOpenScroll({
  isNewThread,
  grew,
  savedScrollTop,
  metrics,
  containerOffsetTop,
  hasUnread,
  firstUnreadOffsetTop,
}: {
  isNewThread: boolean
  /** Whether the message count grew since the last positioning. */
  grew: boolean
  /** Position saved when leaving this thread, or null when not restoring. */
  savedScrollTop: number | null
  metrics: ScrollMetrics
  containerOffsetTop: number
  hasUnread: boolean
  /** offsetTop of the first unread message, or null when none is rendered. */
  firstUnreadOffsetTop: number | null
}): OpenScrollPlan {
  if (savedScrollTop !== null) {
    return { scrollTop: Math.min(savedScrollTop, bottomScrollTop(metrics)), pin: false }
  }

  if (!isNewThread) {
    // A read-state change or a re-render is not a reason to move the view; only
    // a newly arrived message is, and only for a reader already at the bottom.
    if (!grew) return { scrollTop: null, pin: false }
    const distanceFromBottom = metrics.scrollHeight - metrics.scrollTop - metrics.clientHeight
    if (distanceFromBottom > BOTTOM_STICK_PX) return { scrollTop: null, pin: false }
    return { scrollTop: metrics.scrollHeight, pin: false }
  }

  if (firstUnreadOffsetTop === null) {
    // Unread messages the container hasn't rendered yet (a thread still
    // loading): leave the view alone rather than jumping to the bottom of a
    // list that is about to change under it.
    if (hasUnread) return { scrollTop: null, pin: false }
    return { scrollTop: metrics.scrollHeight, pin: false }
  }
  // Bodies still carry their placeholder height here, so the first expansion
  // would otherwise look like "content grew under the fold" and snap the view
  // to the bottom — past the unread messages the reader opened the thread for.
  return { scrollTop: anchorScrollTop(firstUnreadOffsetTop, containerOffsetTop), pin: true }
}
