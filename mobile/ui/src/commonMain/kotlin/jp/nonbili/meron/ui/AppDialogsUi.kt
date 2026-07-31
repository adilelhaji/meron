package jp.nonbili.meron.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp

@Composable
internal fun AddFeedDialog(
    url: String,
    onUrlChange: (String) -> Unit,
    error: String,
    submitting: Boolean,
    onAdd: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("feeds.actions.addFeed")) },
        text = {
            OutlinedTextField(
                value = url,
                onValueChange = onUrlChange,
                label = { Text(tr("feeds.url")) },
                supportingText = error.takeIf { it.isNotBlank() }?.let { message -> { Text(message) } },
                isError = error.isNotBlank(),
                singleLine = true,
                enabled = !submitting,
                colors =
                    OutlinedTextFieldDefaults.colors(
                        unfocusedBorderColor = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.55f),
                    ),
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = {
            TextButton(
                onClick = onAdd,
                enabled = url.isNotBlank() && !submitting,
            ) {
                if (submitting) {
                    CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                } else {
                    Text(tr("common.add"))
                }
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, enabled = !submitting) {
                Text(tr("buttons.cancel"))
            }
        },
    )
}

/**
 * Confirms emptying a Trash or Junk folder. The delete is permanent — there is no
 * Trash left to restore from — so the action always goes through this dialog.
 */
@Composable
internal fun EmptyFolderDialog(
    folderName: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("threads.emptyFolder.confirmTitle", mapOf("folder" to folderName))) },
        text = { Text(tr("threads.emptyFolder.confirmMessage", mapOf("folder" to folderName))) },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text(
                    tr("threads.emptyFolder.confirmButton"),
                    color = MaterialTheme.colorScheme.error,
                )
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(tr("buttons.cancel"))
            }
        },
    )
}

/**
 * Confirms deleting a folder on the server. The folder's mail goes with it and the
 * server keeps no copy, so the action always goes through this dialog.
 */
@Composable
internal fun DeleteFolderDialog(
    folderName: String,
    nested: Int,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("folders.delete.confirmTitle", mapOf("folder" to folderName))) },
        text = {
            Text(
                if (nested > 0) {
                    tr("folders.delete.confirmMessageNested", mapOf("folder" to folderName, "count" to nested))
                } else {
                    tr("folders.delete.confirmMessage", mapOf("folder" to folderName))
                },
            )
        },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text(
                    tr("folders.delete.confirmButton"),
                    color = MaterialTheme.colorScheme.error,
                )
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(tr("buttons.cancel"))
            }
        },
    )
}

/**
 * Offered on the launch after a crash. Nothing has been sent at this point —
 * "send" opens the platform share sheet with the redacted diagnostic log, so
 * the user sees the contents and picks the recipient.
 */
@Composable
internal fun CrashReportDialog(
    summary: String,
    onSend: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("crash.title")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(tr("crash.body"))
                if (summary.isNotBlank()) {
                    Text(
                        summary,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onSend) {
                Text(tr("crash.send"))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(tr("crash.notNow"))
            }
        },
    )
}

@Composable
internal fun KanbanCreateFolderDialog(
    name: String,
    delimiter: String,
    onNameChange: (String) -> Unit,
    onCreate: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("folders.create")) },
        text = {
            OutlinedTextField(
                value = name,
                onValueChange = onNameChange,
                label = { Text(tr("folders.namePlaceholder")) },
                supportingText = { Text(tr("folders.subfolderHint", mapOf("delimiter" to delimiter))) },
                singleLine = true,
                keyboardOptions = nativeTextKeyboardOptions.copy(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { onCreate() }),
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = {
            TextButton(onClick = onCreate) {
                Text(tr("folders.create"))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(tr("buttons.cancel"))
            }
        },
    )
}
