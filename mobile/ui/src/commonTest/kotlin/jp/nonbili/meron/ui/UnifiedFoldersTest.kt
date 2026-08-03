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

    @Test
    fun unifiedColumnsMatchProviderFolderNamesInMailEvents() {
        assertTrue(unifiedColumnMatchesFolder("sent", folders, "Postausgang"))
        assertFalse(unifiedColumnMatchesFolder("sent", folders, "INBOX"))
        assertTrue(unifiedColumnMatchesFolder(STARRED_FOLDER, folders, "INBOX"))
    }
}
