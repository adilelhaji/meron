package jp.nonbili.meron.ui

import jp.nonbili.meron.shared.ThreadSummary
import kotlin.test.Test
import kotlin.test.assertEquals

class ThreadFilterTest {
    @Test
    fun starredFilterKeepsFeedsContainingStarredItemsWithoutStarringTheFeed() {
        val feed =
            ThreadSummary(
                id = "rss-account#rss#feed-1",
                accountId = "rss-account",
                folder = "feeds",
                subject = "Example Feed",
                sender = "Example Feed",
                starred = false,
                hasStarredItems = true,
            )

        assertEquals(listOf(feed), listOf(feed).filteredKanbanThreads(FilterMode.Starred, ""))
    }
}
