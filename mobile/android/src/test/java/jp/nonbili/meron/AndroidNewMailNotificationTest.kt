package jp.nonbili.meron

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class AndroidNewMailNotificationTest {
    private fun item(
        uid: Long = 0,
        from: String = "",
        subject: String = "",
        preview: String = "",
        threadKey: String = "",
    ) = NewMailItem(uid = uid, from = from, subject = subject, preview = preview, threadKey = threadKey, date = 0)

    @Test
    fun childTextShowsSubjectAndBodySnippet() {
        assertEquals(
            "Lunch? - Are we still on for Friday",
            newMailChildText("Lunch?", "Are we still on for Friday"),
        )
        // Expanded form puts the body under the subject rather than after it.
        assertEquals(
            "Lunch?\nAre we still on for Friday",
            newMailChildBigText("Lunch?", "Are we still on for Friday"),
        )
    }

    @Test
    fun childTextDegradesWhenTheBodyIsNotCachedYet() {
        assertEquals("Lunch?", newMailChildText("Lunch?", ""))
        assertEquals("Lunch?", newMailChildBigText("Lunch?", "   "))
        // Nothing at all to show still beats a blank notification line.
        assertEquals("New mail arrived", newMailChildText("", ""))
        assertEquals("New mail arrived", newMailChildBigText(" ", ""))
    }

    @Test
    fun childTitleFallsBackFromSenderToAccount() {
        assertEquals("Aki", newMailChildTitle("Aki", "me@example.com"))
        assertEquals("me@example.com", newMailChildTitle("  ", "me@example.com"))
        assertEquals("New mail", newMailChildTitle("", ""))
    }

    @Test
    fun summaryLinesAndCountReadAsGmailsDo() {
        assertEquals("Aki - Lunch?", newMailInboxLine("Aki", "Lunch?"))
        assertEquals("Aki", newMailInboxLine("Aki", ""))
        assertEquals("1 new message", newMailSummaryText(1))
        assertEquals("3 new messages", newMailSummaryText(3))
    }

    @Test
    fun notificationIdIsStablePerMessageAndDistinctPerAccount() {
        val first = item(uid = 42, subject = "Lunch?")
        // Re-posting the same mail (push racing a periodic refresh) updates the
        // one notification instead of stacking a duplicate.
        assertEquals(
            newMailNotificationId("me@example.com", first),
            newMailNotificationId("me@example.com", item(uid = 42, subject = "Lunch? (edited)")),
        )
        assertNotEquals(
            newMailNotificationId("me@example.com", first),
            newMailNotificationId("work@example.com", first),
        )
        assertNotEquals(
            newMailNotificationId("me@example.com", first),
            newMailNotificationId("me@example.com", item(uid = 43, subject = "Lunch?")),
        )
        // Same-account arrivals never collide with that account's summary.
        assertNotEquals(newMailSummaryId("me@example.com"), newMailNotificationId("me@example.com", first))
    }

    @Test
    fun uidlessPayloadsKeyOffTheThread() {
        val feedItem = item(threadKey = "feed-1", subject = "Release notes")
        assertEquals(
            newMailNotificationId("rss", feedItem),
            newMailNotificationId("rss", item(threadKey = "feed-1", subject = "Release notes")),
        )
        assertNotEquals(
            newMailNotificationId("rss", feedItem),
            newMailNotificationId("rss", item(threadKey = "feed-2", subject = "Release notes")),
        )
    }

    @Test
    fun groupKeySeparatesAccounts() {
        assertNotEquals(newMailGroupKey("me@example.com"), newMailGroupKey("work@example.com"))
    }
}
