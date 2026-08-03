package jp.nonbili.meron.ui

import jp.nonbili.meron.shared.FolderSummary
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class UnifiedFoldersTest {
    private val folders =
        listOf(
            FolderSummary(accountId = "acc", name = "INBOX", role = "inbox"),
            FolderSummary(accountId = "acc", name = "Postausgang", role = "sent"),
        )

    @Test
    fun resolvesRolesToProviderMailboxNames() {
        assertEquals("Postausgang", unifiedAccountFolder(folders, "sent"))
    }

    @Test
    fun inboxHasABootstrapFallbackButMissingRolesAreSkipped() {
        assertEquals(INBOX_FOLDER, unifiedAccountFolder(emptyList(), "inbox"))
        assertNull(unifiedAccountFolder(folders, "archive"))
    }

    // Starred is the one switcher entry that is not a per-account mailbox, so
    // it must never resolve to one — the inbox fallback would silently show the
    // wrong list.
    @Test
    fun starredSitsNextToTheInboxInTheSwitcherButIsNoMailboxRole() {
        assertEquals(listOf(INBOX_FOLDER, STARRED_FOLDER), UNIFIED_VIEW_ROLES.take(2))
        assertTrue(isUnifiedStarredFolder("Starred"))
        assertFalse(isUnifiedStarredFolder("sent"))
        assertFalse(UNIFIED_FOLDER_ROLES.contains(STARRED_FOLDER))
    }

    @Test
    fun unifiedColumnsMatchProviderFolderNamesInMailEvents() {
        assertTrue(unifiedColumnMatchesFolder("sent", folders, "Postausgang"))
        assertFalse(unifiedColumnMatchesFolder("sent", folders, "INBOX"))
        assertTrue(unifiedColumnMatchesFolder(STARRED_FOLDER, folders, "INBOX"))
    }
}
