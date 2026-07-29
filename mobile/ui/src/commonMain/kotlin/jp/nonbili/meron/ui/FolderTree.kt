package jp.nonbili.meron.ui

import jp.nonbili.meron.shared.FolderSummary

/**
 * A folder in the hierarchy built from IMAP path names. [folder] is null for a
 * structural node — a path segment that only exists as a parent of real folders.
 */
internal data class FolderTreeNode(
    val name: String,
    val folder: FolderSummary?,
    val children: List<FolderTreeNode>,
)

/** A tree node paired with its depth, for rendering the tree as a flat list. */
internal data class FolderTreeRow(
    val node: FolderTreeNode,
    val depth: Int,
)

/** Pick the hierarchy delimiter the folder names appear to use. */
internal fun folderTreeDelimiter(folders: List<FolderSummary>): String =
    when {
        folders.any { it.name.contains('/') } -> "/"
        folders.any { it.name.contains('.') } -> "."
        else -> "/"
    }

private class MutableFolderTreeNode(
    val name: String,
    var folder: FolderSummary? = null,
    val children: MutableList<MutableFolderTreeNode> = mutableListOf(),
) {
    fun toNode(): FolderTreeNode = FolderTreeNode(name, folder, children.map { it.toNode() })
}

/** Group folders into a tree by splitting their names on the hierarchy delimiter. */
internal fun buildFolderTree(folders: List<FolderSummary>): List<FolderTreeNode> {
    val delimiter = folderTreeDelimiter(folders)
    val roots = mutableListOf<MutableFolderTreeNode>()
    val byPath = mutableMapOf<String, MutableFolderTreeNode>()
    folders.forEach { folder ->
        val segments =
            folder.name
                .split(delimiter)
                .filter { it.isNotEmpty() }
                .ifEmpty { listOf(folder.name) }
        var siblings = roots
        var path = ""
        segments.forEachIndexed { index, segment ->
            path = if (path.isEmpty()) segment else "$path$delimiter$segment"
            val node =
                byPath.getOrPut(path) {
                    MutableFolderTreeNode(segment).also { siblings.add(it) }
                }
            // The final segment is the real folder; intermediates may be structural.
            if (index == segments.lastIndex) node.folder = folder
            siblings = node.children
        }
    }
    return roots.map { it.toNode() }
}

/** Depth-first flattening of [nodes], so the tree can be laid out as menu rows. */
internal fun flattenFolderTree(
    nodes: List<FolderTreeNode>,
    depth: Int = 0,
): List<FolderTreeRow> =
    nodes.flatMap { node ->
        listOf(FolderTreeRow(node, depth)) + flattenFolderTree(node.children, depth + 1)
    }
