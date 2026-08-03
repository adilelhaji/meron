package jp.nonbili.meron.ui

import jp.nonbili.meron.shared.FolderSummary
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

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
}
