package jp.nonbili.meron.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import jp.nonbili.meron.shared.FolderSummary

/**
 * A folder name that doubles as a picker: tapping it lists the other folders of
 * the same account, so the surface showing it (a kanban column, the mail list)
 * can be pointed elsewhere without being torn down and rebuilt.
 */
@Composable
internal fun FolderSwitcher(
    label: String,
    folders: List<FolderSummary>,
    currentFolderId: String,
    onRequestFolders: () -> Unit,
    onSelectFolder: (String) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    /** Folders already shown elsewhere (e.g. another column) and so not offered. */
    takenFolderIds: Set<String> = emptySet(),
    fontSize: TextUnit = 12.sp,
    fontWeight: FontWeight = FontWeight.SemiBold,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Box(modifier) {
        Row(
            Modifier
                .clip(RoundedCornerShape(6.dp))
                .clickable(enabled = enabled) {
                    onRequestFolders()
                    menuOpen = true
                }.padding(horizontal = 2.dp, vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                label,
                fontWeight = fontWeight,
                fontSize = fontSize,
                modifier = Modifier.weight(1f, fill = false),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (enabled) {
                Icon(
                    Icons.Filled.KeyboardArrowDown,
                    contentDescription = tr("kanban.actions.switchFolder"),
                    modifier = Modifier.size(15.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            if (folders.isEmpty()) {
                DropdownMenuItem(text = { Text(tr("folders.loading")) }, onClick = {}, enabled = false)
            }
            // Nested folders are indented under their parent, the same hierarchy
            // the add-column dialog shows.
            val folderRows = remember(folders) { flattenFolderTree(buildFolderTree(folders)) }
            folderRows.forEach { row ->
                val folder = row.node.folder
                val current = folder != null && kanbanFolderIdsEqual(folder.name, currentFolderId)
                val taken =
                    folder != null &&
                        !current &&
                        takenFolderIds.any { kanbanFolderIdsEqual(folder.name, it) }
                DropdownMenuItem(
                    modifier = Modifier.padding(start = (row.depth * 14).dp),
                    text = {
                        Text(
                            row.node.name.replaceFirstChar { it.uppercase() },
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            fontWeight = if (current) FontWeight.SemiBold else FontWeight.Normal,
                        )
                    },
                    leadingIcon = {
                        Icon(
                            when {
                                current -> Icons.Filled.Check
                                folder != null -> folderIcon(folder)
                                else -> folderIcon(row.node.name)
                            },
                            contentDescription = null,
                            tint =
                                if (current) {
                                    MaterialTheme.colorScheme.primary
                                } else {
                                    MaterialTheme.colorScheme.onSurfaceVariant
                                },
                        )
                    },
                    // Structural nodes (no folder of their own) are labels only.
                    enabled = folder != null && !taken && !current,
                    onClick = {
                        menuOpen = false
                        folder?.let { onSelectFolder(it.name) }
                    },
                )
            }
        }
    }
}
