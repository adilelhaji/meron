package jp.nonbili.meron.shared

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class NotificationThreadIdsTest {
    @Test
    fun headerKeysAreBase64UrlEncodedBehindTheThreadPrefix() {
        // Matches the composite core's parse_thread_id splits back apart:
        // account, folder, then "t." + base64url of the thread key.
        assertEquals(
            "me@example.com#INBOX#t.dG9waWM",
            notificationThreadId("me@example.com", "INBOX", "topic"),
        )
    }

    @Test
    fun uidKeysAreWrittenAsTheBareUid() {
        assertEquals(
            "me@example.com#INBOX#4821",
            notificationThreadId("me@example.com", "INBOX", "uid:4821"),
        )
    }

    @Test
    fun theInboxIsNormalizedToTheNameImapUses() {
        // The UI's own folder id is lowercase; IMAP would not recognize it.
        assertEquals(
            notificationThreadId("me@example.com", "INBOX", "topic"),
            notificationThreadId("me@example.com", "inbox", "topic"),
        )
    }

    @Test
    fun otherFoldersKeepTheirCase() {
        assertEquals(
            "me@example.com#Archive#t.dG9waWM",
            notificationThreadId("me@example.com", "Archive", "topic"),
        )
    }

    @Test
    fun onlyHeaderDerivedKeysSurviveAMove() {
        // A uid names a different message in the target mailbox, so an undo
        // keyed on it would move whatever now holds that uid.
        assertFalse(notificationThreadKeyIsStableAcrossMove("uid:4821"))
        assertFalse(notificationThreadKeyIsStableAcrossMove(""))
        assertTrue(notificationThreadKeyIsStableAcrossMove("topic"))
        assertTrue(notificationThreadKeyIsStableAcrossMove("<abc@example.com>#Re: lunch"))
    }
}
