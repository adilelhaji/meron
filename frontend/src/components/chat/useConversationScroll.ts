import { useCallback, useEffect, useLayoutEffect, useRef } from 'react'
import { useValue } from '@legendapp/state/react'
import { markMessagesRead } from '../../states/mail'
import { thread$ } from '../../states/thread'
import type { Message } from '../../types'
import {
  anchorScrollTop,
  isUserScroll,
  OPEN_ANCHOR_WINDOW_MS,
  resolveOpenScroll,
  resolveResizeScrollTop,
  type ScrollMetrics,
} from './conversationScroll'

function readScrollMetrics(container: HTMLElement): ScrollMetrics {
  return {
    scrollTop: container.scrollTop,
    scrollHeight: container.scrollHeight,
    clientHeight: container.clientHeight,
  }
}

// Owns the conversation scroll container and all of its positioning behaviour:
// restoring scroll when returning to a thread, autoscrolling on new messages,
// jumping to the first unread on open, and marking rendered messages read as they
// scroll past. Returns the refs the message list wires up plus the scroll
// handler. `unreadKey` changes whenever any message's unread flag flips.
export function useConversationScroll(
  activeThreadId: string,
  messages: Message[],
  activeTab: string,
  unreadKey: string,
) {
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const bottomAnchorRef = useRef<HTMLDivElement | null>(null)
  const messagesWrapperRef = useRef<HTMLDivElement | null>(null)
  const lastScrollHeightRef = useRef(0)
  const markingMessageIdsRef = useRef(new Set<string>())
  const conversationScrollTopRef = useRef(new Map<string, number>())
  const pendingScrollRestoreThreadRef = useRef('')
  // Thread we've already done the one-time open positioning for, and the message
  // count at the last positioning — used to tell "thread opened" / "new message
  // arrived" apart from "read state changed" so we don't yank the user's scroll.
  const positionedThreadRef = useRef('')
  const messageCountRef = useRef(0)
  // Message the last positioning landed on — a starred-list jump, or the first
  // unread on open. While set, the ResizeObserver below re-anchors to it (instead
  // of snapping to the bottom) so bodies growing from their placeholder height
  // don't push it out of view; released shortly after the jump.
  const pinnedMessageIdRef = useRef('')
  const pinReleaseTimerRef = useRef(0)

  // Last position we assigned ourselves, so scroll events we caused can be told
  // apart from the reader's — see isUserScroll.
  const expectedScrollTopRef = useRef<number | null>(null)

  const applyScrollTop = useCallback((container: HTMLElement, scrollTop: number) => {
    container.scrollTop = scrollTop
    // Read it back: the browser clamps to the scrollable range, and the clamped
    // value is what the scroll event will report.
    expectedScrollTopRef.current = container.scrollTop
  }, [])

  // Same bookkeeping for the message list's own repositioning (holding the view
  // still while older history is prepended): without it that assignment reads as
  // the reader scrolling and drops the anchor.
  const setScrollTop = useCallback(
    (scrollTop: number) => {
      const container = scrollRef.current
      if (container) applyScrollTop(container, scrollTop)
    },
    [applyScrollTop],
  )

  const pinMessage = useCallback((messageId: string) => {
    pinnedMessageIdRef.current = messageId
    window.clearTimeout(pinReleaseTimerRef.current)
    pinReleaseTimerRef.current = window.setTimeout(() => {
      pinnedMessageIdRef.current = ''
    }, OPEN_ANCHOR_WINDOW_MS)
  }, [])

  useEffect(() => () => window.clearTimeout(pinReleaseTimerRef.current), [])

  const pendingScrollMessageId = useValue(thread$.pendingScrollMessageId)

  const saveConversationScroll = useCallback(
    (restoreOnReturn = false) => {
      const container = scrollRef.current
      if (!container || !activeThreadId) return
      conversationScrollTopRef.current.set(activeThreadId, container.scrollTop)
      if (restoreOnReturn) {
        pendingScrollRestoreThreadRef.current = activeThreadId
      }
    },
    [activeThreadId],
  )

  const maybeMarkRead = useCallback(() => {
    const container = scrollRef.current
    if (!container || !activeThreadId) return
    const hasUnread = messages.some((message) => message.thread_id === activeThreadId && message.unread)
    if (!hasUnread) return

    const containerRect = container.getBoundingClientRect()
    const visibleMessageIds = Array.from(container.querySelectorAll<HTMLElement>('[data-unread="true"]'))
      .filter((element) => {
        const rect = element.getBoundingClientRect()
        return rect.top < containerRect.bottom && rect.bottom > containerRect.top
      })
      .map((element) => element.dataset.messageId)
      .filter((id): id is string => !!id && !markingMessageIdsRef.current.has(id))

    if (visibleMessageIds.length === 0) return
    for (const id of visibleMessageIds) {
      markingMessageIdsRef.current.add(id)
    }
    void markMessagesRead(activeThreadId, visibleMessageIds).catch((error) => {
      for (const id of visibleMessageIds) {
        markingMessageIdsRef.current.delete(id)
      }
      console.error('Failed to mark visible messages read:', error)
    })
  }, [activeThreadId, messages])

  const handleConversationScroll = useCallback(() => {
    const container = scrollRef.current
    // The reader moving the view — including by dragging the scrollbar, which
    // dispatches no mouse events here — outranks the settle-window anchor.
    if (container && pinnedMessageIdRef.current && isUserScroll(container.scrollTop, expectedScrollTopRef.current)) {
      pinnedMessageIdRef.current = ''
    }
    saveConversationScroll()
    maybeMarkRead()
  }, [maybeMarkRead, saveConversationScroll])

  useLayoutEffect(() => {
    return () => {
      if (activeTab === '') {
        saveConversationScroll(true)
      }
    }
  }, [activeTab, saveConversationScroll])

  useEffect(() => {
    markingMessageIdsRef.current.clear()
  }, [activeThreadId, unreadKey])

  // Attach before paint and before child HtmlFrame effects begin reporting their
  // measured heights. On a cold open, attaching in a passive effect can miss the
  // first placeholder-to-content resize and leave a fully read thread at the top
  // instead of keeping its initial newest-message anchor.
  useLayoutEffect(() => {
    const container = scrollRef.current
    const wrapper = messagesWrapperRef.current
    if (activeTab !== '' || !container || !wrapper || !activeThreadId) return

    lastScrollHeightRef.current = container.scrollHeight

    const observer = new ResizeObserver(() => {
      const pinned = pinnedMessageIdRef.current
        ? container.querySelector<HTMLElement>(`[data-message-id="${CSS.escape(pinnedMessageIdRef.current)}"]`)
        : null
      const scrollTop = resolveResizeScrollTop({
        metrics: readScrollMetrics(container),
        previousScrollHeight: lastScrollHeightRef.current,
        containerOffsetTop: container.offsetTop,
        pinnedOffsetTop: pinned ? pinned.offsetTop : null,
      })
      if (scrollTop !== null) {
        applyScrollTop(container, scrollTop)
      }
      lastScrollHeightRef.current = container.scrollHeight
      maybeMarkRead()
    })

    observer.observe(wrapper)
    return () => observer.disconnect()
  }, [activeTab, activeThreadId, messages, applyScrollTop])

  useLayoutEffect(() => {
    const container = scrollRef.current
    if (activeTab !== '' || !container || !activeThreadId || messages.length === 0) return
    if (messages.some((message) => message.thread_id !== activeThreadId)) return

    // A starred-list jump: scroll to the requested message and flash its ring.
    // Consumed exactly once; if the message isn't in the loaded page (older than
    // the first page), fall through to the normal open positioning.
    if (pendingScrollMessageId) {
      thread$.pendingScrollMessageId.set('')
      const target = container.querySelector<HTMLElement>(`[data-message-id="${CSS.escape(pendingScrollMessageId)}"]`)
      if (target) {
        positionedThreadRef.current = activeThreadId
        messageCountRef.current = messages.length
        pendingScrollRestoreThreadRef.current = ''
        pinMessage(pendingScrollMessageId)
        applyScrollTop(container, anchorScrollTop(target.offsetTop, container.offsetTop))
        thread$.flashMessageId.set(pendingScrollMessageId)
        window.setTimeout(() => {
          if (thread$.flashMessageId.peek() === pendingScrollMessageId) {
            thread$.flashMessageId.set('')
          }
        }, OPEN_ANCHOR_WINDOW_MS)
        maybeMarkRead()
        return
      }
    }

    const isNewThread = positionedThreadRef.current !== activeThreadId
    const grew = messages.length > messageCountRef.current
    messageCountRef.current = messages.length
    let savedScrollTop: number | null = null
    if (pendingScrollRestoreThreadRef.current === activeThreadId) {
      pendingScrollRestoreThreadRef.current = ''
      savedScrollTop = conversationScrollTopRef.current.get(activeThreadId) ?? null
    }

    const hasUnread = messages.some((message) => message.unread)
    const firstUnread = hasUnread ? container.querySelector<HTMLElement>('[data-unread="true"]') : null
    if (isNewThread) {
      positionedThreadRef.current = activeThreadId
    }

    const plan = resolveOpenScroll({
      isNewThread,
      grew,
      savedScrollTop,
      metrics: readScrollMetrics(container),
      containerOffsetTop: container.offsetTop,
      hasUnread,
      firstUnreadOffsetTop: firstUnread ? firstUnread.offsetTop : null,
    })
    if (plan.pin && firstUnread?.dataset.messageId) {
      pinMessage(firstUnread.dataset.messageId)
    }
    if (plan.scrollTop !== null) {
      applyScrollTop(container, plan.scrollTop)
    }
    maybeMarkRead()
  }, [
    activeTab,
    activeThreadId,
    messages.length,
    unreadKey,
    maybeMarkRead,
    pendingScrollMessageId,
    pinMessage,
    applyScrollTop,
  ])

  return { scrollRef, bottomAnchorRef, messagesWrapperRef, handleConversationScroll, maybeMarkRead, setScrollTop }
}
