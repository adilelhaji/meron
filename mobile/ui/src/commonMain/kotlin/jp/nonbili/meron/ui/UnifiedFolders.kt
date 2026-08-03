package jp.nonbili.meron.ui

import androidx.compose.runtime.Composable
import jp.nonbili.meron.shared.FolderSummary

/**
 * The folders the unified view offers, in the same order a single account lists
 * its own — so switching between an account and the unified view keeps the same
 * muscle memory. These are roles, not folder names: "Sent" here means each
 * account's own Sent, which the core resolves per account, leaving out accounts
 * whose server has no such mailbox.
 *
 * Starred is deliberately absent: unlike desktop, mobile already surfaces every
 * starred item across accounts as its own tab, so listing it here too would be a
 * second door to the same screen.
 */
internal val UNIFIED_FOLDER_ROLES = listOf(INBOX_FOLDER, "sent", "drafts", "archive", "junk", "trash")

/** Locale keys for the role names, shared with the desktop folder switcher. */
private val UNIFIED_ROLE_LABEL_KEYS =
    mapOf(
        INBOX_FOLDER to "folders.roles.inbox",
        "sent" to "folders.roles.sent",
        "drafts" to "folders.roles.drafts",
        "archive" to "folders.roles.archive",
        "junk" to "folders.roles.junk",
        "trash" to "folders.roles.trash",
    )

/** A selection that is not a unified role falls back to the inbox. */
internal fun unifiedFolderRole(folderId: String): String = UNIFIED_FOLDER_ROLES.firstOrNull { it.equals(folderId, ignoreCase = true) } ?: INBOX_FOLDER

/** Resolve a unified role to the real mailbox name used by one account. */
internal fun unifiedAccountFolder(
    folders: List<FolderSummary>,
    folderRole: String,
): String? {
    val role = unifiedFolderRole(folderRole)
    return folders.firstOrNull { it.role.equals(role, ignoreCase = true) }?.name
        ?: INBOX_FOLDER.takeIf { role == INBOX_FOLDER }
}

/**
 * The unified view's synthetic folder list. The folder *name* is the role, so a
 * picked entry round-trips straight back as the requested role.
 */
@Composable
internal fun unifiedFolders(): List<FolderSummary> =
    UNIFIED_FOLDER_ROLES.map { role ->
        FolderSummary(
            accountId = UNIFIED_ACCOUNT_ID,
            name = role,
            role = role,
            displayName = tr(UNIFIED_ROLE_LABEL_KEYS.getValue(role)),
        )
    }

/** The unified view's label for [folderId], e.g. "Sent". */
@Composable
internal fun unifiedFolderLabel(folderId: String): String = tr(UNIFIED_ROLE_LABEL_KEYS.getValue(unifiedFolderRole(folderId)))

/**
 * A unified Kanban column's own name. Unlike the mail list — where the unified
 * mailbox is the whole screen and the folder name alone is unambiguous — a
 * column sits beside single-account columns whose folders are called the same
 * thing, so it has to carry "Unified" itself. Spelled out per role rather than
 * composed from "Unified" + a folder name, which no language agrees on.
 */
private val UNIFIED_COLUMN_LABEL_KEYS =
    mapOf(
        INBOX_FOLDER to "kanban.columns.unifiedInbox",
        STARRED_FOLDER to "kanban.columns.unifiedStarred",
        "sent" to "kanban.columns.unifiedSent",
        "drafts" to "kanban.columns.unifiedDrafts",
        "archive" to "kanban.columns.unifiedArchive",
        "junk" to "kanban.columns.unifiedJunk",
        "trash" to "kanban.columns.unifiedTrash",
    )

@Composable
internal fun unifiedColumnLabel(folderId: String): String = tr(UNIFIED_COLUMN_LABEL_KEYS.getValue(if (folderId.equals(STARRED_FOLDER, ignoreCase = true)) STARRED_FOLDER else unifiedFolderRole(folderId)))

internal fun unifiedColumnMatchesFolder(
    columnFolderId: String,
    accountFolders: List<FolderSummary>,
    eventFolderId: String,
): Boolean {
    if (columnFolderId.equals(STARRED_FOLDER, ignoreCase = true)) return true
    val role = unifiedFolderRole(columnFolderId)
    if (eventFolderId.isBlank()) return role == INBOX_FOLDER
    val accountFolder = unifiedAccountFolder(accountFolders, role) ?: return false
    return kanbanFolderIdsEqual(accountFolder, eventFolderId)
}

/**
 * The roles a unified *Kanban column* offers. Starred is included here though
 * the mail list leaves it out: the mail list has a dedicated Starred tab to
 * reach it by, a board column has none, and the column picker no longer offers
 * a separate starred entry. Ordered as on desktop, starred next to the inbox.
 */
internal val UNIFIED_COLUMN_ROLES = listOf(INBOX_FOLDER, STARRED_FOLDER) + UNIFIED_FOLDER_ROLES.filterNot { it == INBOX_FOLDER }

/**
 * The folder list a unified column's switcher shows. As with [unifiedFolders]
 * the folder *name* is the role, so a picked entry round-trips straight back as
 * the requested role.
 */
@Composable
internal fun unifiedColumnFolders(): List<FolderSummary> =
    UNIFIED_COLUMN_ROLES.map { role ->
        FolderSummary(
            accountId = UNIFIED_ACCOUNT_ID,
            name = role,
            role = role,
            // Starred has no folder-role name of its own; it reuses the filter
            // label, the same string the desktop switcher shows.
            displayName = if (role == STARRED_FOLDER) tr("filters.starred") else tr(UNIFIED_ROLE_LABEL_KEYS.getValue(role)),
        )
    }
