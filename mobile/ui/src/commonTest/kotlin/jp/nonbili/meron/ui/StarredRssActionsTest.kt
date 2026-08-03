package jp.nonbili.meron.ui

import jp.nonbili.meron.shared.ThreadSummary
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class StarredRssActionsTest {
    @Test
    fun starredRssRowTargetsOnlyItsItem() {
        val threadId = "rss-account#rss#feed-a"
        val row =
            ThreadSummary(
                id = "$threadId#item-one",
                threadId = threadId,
                accountId = "rss-account",
                folder = "rss",
                subject = "Item one",
                sender = "Feed A",
            )

        assertEquals(threadId, row.backendThreadId())
        assertEquals(listOf("$threadId#item-one"), row.rssItemKeys())
    }

    @Test
    fun ordinaryRssRowStillTargetsTheWholeFeed() {
        val threadId = "rss-account#rss#feed-a"
        val row =
            ThreadSummary(
                id = threadId,
                threadId = threadId,
                accountId = "rss-account",
                folder = "rss",
                subject = "Feed A",
                sender = "Feed A",
            )

        assertEquals(threadId, row.backendThreadId())
        assertTrue(row.rssItemKeys().isEmpty())
    }
}
