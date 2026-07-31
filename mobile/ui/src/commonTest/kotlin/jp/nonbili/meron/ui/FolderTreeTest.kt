package jp.nonbili.meron.ui

import jp.nonbili.meron.shared.FolderSummary
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class FolderTreeTest {
    private fun folder(name: String) = FolderSummary(accountId = "acc1", name = name)

    @Test
    fun nestsFoldersUnderTheirParentPath() {
        val tree = buildFolderTree(listOf(folder("INBOX"), folder("Work/Clients"), folder("Work/Clients/Acme"), folder("Work")))

        assertEquals(listOf("INBOX", "Work"), tree.map { it.name })
        val work = tree.last()
        assertEquals(listOf("Clients"), work.children.map { it.name })
        assertEquals(
            listOf("Acme"),
            work.children
                .first()
                .children
                .map { it.name },
        )
        assertEquals(
            "Work/Clients/Acme",
            work.children
                .first()
                .children
                .first()
                .folder
                ?.name,
        )
    }

    @Test
    fun prefersTheServerReportedDelimiterOverInference() {
        val folders =
            listOf(
                FolderSummary(accountId = "acc1", name = "INBOX", delimiter = "."),
                FolderSummary(accountId = "acc1", name = "INBOX.Work", delimiter = "."),
            )

        assertEquals(".", folderTreeDelimiter(folders))
        val inbox = buildFolderTree(folders).single()
        assertEquals(listOf("Work"), inbox.children.map { it.name })
    }

    @Test
    fun infersTheDelimiterWhenTheServerReportsNone() {
        assertEquals("/", folderTreeDelimiter(listOf(folder("Work/Clients"))))
        assertEquals(".", folderTreeDelimiter(listOf(folder("INBOX.Work"))))
        assertEquals("/", folderTreeDelimiter(listOf(folder("INBOX"))))
    }

    @Test
    fun labelsNodesWithTheDecodedNameAndKeepsWirePathsForIdentity() {
        val encoded = FolderSummary(accountId = "acc1", name = "Work/gds-&AOQA5A--envoy&AOk-s", displayName = "Work/gds-ää-envoyés")

        val work = buildFolderTree(listOf(folder("Work"), encoded)).single()

        assertEquals(listOf("gds-ää-envoyés"), work.children.map { it.name })
        assertEquals(
            "Work/gds-&AOQA5A--envoy&AOk-s",
            work.children
                .single()
                .folder
                ?.name,
        )
    }

    @Test
    fun keepsMissingIntermediatesAsStructuralNodes() {
        val tree = buildFolderTree(listOf(folder("Work/Clients/Acme")))

        val work = tree.single()
        assertNull(work.folder)
        assertEquals(
            "Acme",
            work.children
                .single()
                .children
                .single()
                .name,
        )
    }

    @Test
    fun infersDotDelimiterWhenNoSlashIsUsed() {
        val tree = buildFolderTree(listOf(folder("INBOX.Sent"), folder("INBOX.Drafts")))

        assertEquals(listOf("INBOX"), tree.map { it.name })
        assertEquals(listOf("Sent", "Drafts"), tree.single().children.map { it.name })
    }

    @Test
    fun flattensDepthFirstWithDepths() {
        val rows = flattenFolderTree(buildFolderTree(listOf(folder("Work"), folder("Work/Acme"), folder("Personal"))))

        assertEquals(listOf("Work" to 0, "Acme" to 1, "Personal" to 0), rows.map { it.node.name to it.depth })
        assertTrue(rows.all { it.node.folder != null })
    }
}
