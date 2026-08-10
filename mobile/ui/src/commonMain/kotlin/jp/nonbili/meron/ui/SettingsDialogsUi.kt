package jp.nonbili.meron.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

@Composable
internal fun ThemePickerDialog(
    current: AppAppearanceMode,
    onSelect: (AppAppearanceMode) -> Unit,
    onDismiss: () -> Unit,
) {
    // Light and dark sections of preview swatches, mirroring the desktop
    // ThemeDialog grid.
    val selectableModes = AppAppearanceMode.entries.filterNot { it == AppAppearanceMode.System }
    val (darkModes, lightModes) = selectableModes.partition { themePreviewColors(it).dark }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("common.theme")) },
        text = {
            val lightLabel = tr("theme.light")
            val darkLabel = tr("theme.dark")
            LazyColumn(
                modifier = Modifier.heightIn(max = 480.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                themeSwatchSection(lightLabel, lightModes, current, onSelect, onDismiss)
                themeSwatchSection(darkLabel, darkModes, current, onSelect, onDismiss)
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(tr("buttons.done")) } },
    )
}

/** A labelled section of theme swatches, laid out two per row. */
private fun LazyListScope.themeSwatchSection(
    label: String,
    modes: List<AppAppearanceMode>,
    current: AppAppearanceMode,
    onSelect: (AppAppearanceMode) -> Unit,
    onDismiss: () -> Unit,
) {
    item {
        Text(
            label,
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(bottom = 2.dp),
        )
    }
    items(modes.chunked(2)) { row ->
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            row.forEach { mode ->
                ThemeSwatch(
                    mode = mode,
                    selected = mode == current,
                    onSelect = {
                        onSelect(mode)
                        onDismiss()
                    },
                    modifier = Modifier.weight(1f),
                )
            }
            if (row.size == 1) Spacer(Modifier.weight(1f))
        }
    }
}

/**
 * Message text size picker: a slider over the size steps above a sample drawn
 * at the size it would pick. The setting is applied as the slider moves rather
 * than on dismiss, so the mail behind the dialog resizes with the sample and
 * there is nothing to confirm.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun MessageTextSizeDialog(
    scale: Int,
    onScaleChange: (Int) -> Unit,
    onDismiss: () -> Unit,
) {
    val steps = MESSAGE_FONT_SCALE_STEPS
    val index = steps.indexOf(coerceMessageFontScale(scale)).coerceAtLeast(0)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("settings.appearance.messageTextSize")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                // Fixed height: the sample reflows as it grows, and a box that
                // resized with it would shift the slider under the finger.
                Surface(
                    color = MaterialTheme.colorScheme.surfaceVariant,
                    shape = RoundedCornerShape(10.dp),
                ) {
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .height(132.dp)
                            .verticalScroll(rememberScrollState())
                            .padding(12.dp),
                    ) {
                        Text(
                            tr("settings.appearance.textSizeSample"),
                            style = messageBodyTextStyle(MaterialTheme.typography.bodyLarge, scale),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text("A", style = MaterialTheme.typography.bodySmall)
                    Slider(
                        value = index.toFloat(),
                        // Slider snaps to the stops itself, but a snapped float
                        // can land a hair under its stop, which truncation
                        // would read as the one below.
                        onValueChange = { onScaleChange(steps[it.roundToInt().coerceIn(0, steps.lastIndex)]) },
                        valueRange = 0f..steps.lastIndex.toFloat(),
                        // One stop per step, minus the two endpoints.
                        steps = steps.size - 2,
                        modifier = Modifier.weight(1f),
                    )
                    Text("A", style = MaterialTheme.typography.titleLarge)
                }
                Text(
                    trf("settings.appearance.textSizeValue", steps[index]),
                    modifier = Modifier.align(Alignment.CenterHorizontally),
                    color = MaterialTheme.colorScheme.primary,
                )
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(tr("buttons.done")) } },
        dismissButton = {
            if (steps[index] != DEFAULT_MESSAGE_FONT_SCALE) {
                TextButton(onClick = { onScaleChange(DEFAULT_MESSAGE_FONT_SCALE) }) {
                    Text(tr("common.resetToDefault"))
                }
            }
        },
    )
}

@Composable
internal fun LanguagePickerDialog(
    currentTag: String,
    onSelect: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("settings.language.label")) },
        text = {
            LazyColumn(modifier = Modifier.heightIn(max = 480.dp)) {
                item {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clickable {
                                onSelect("")
                                onDismiss()
                            }.padding(vertical = 2.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = currentTag.isBlank(), onClick = {
                            onSelect("")
                            onDismiss()
                        })
                        Text(tr("settings.language.system"), modifier = Modifier.padding(start = 8.dp))
                    }
                }
                items(supportedAppLanguageTags) { tag ->
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clickable {
                                onSelect(tag)
                                onDismiss()
                            }.padding(vertical = 2.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = tag == currentTag, onClick = {
                            onSelect(tag)
                            onDismiss()
                        })
                        Text(appLanguageDisplayName(tag), modifier = Modifier.padding(start = 8.dp))
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(tr("buttons.done")) } },
    )
}

/**
 * Passphrase prompt shared by both halves of backup/restore.
 *
 * [BackupPassphraseMode.Export] collects a passphrase (and whether to include
 * account passwords); [BackupPassphraseMode.Restore] collects the passphrase
 * for a file that turned out to be encrypted. The caller owns the core call, so
 * this composable only gathers input.
 */
@Composable
internal fun BackupPassphraseDialog(
    mode: BackupPassphraseMode,
    busy: Boolean,
    error: String,
    onDismiss: () -> Unit,
    onConfirm: (passphrase: String, includeSecrets: Boolean) -> Unit,
) {
    var passphrase by remember { mutableStateOf("") }
    var confirmation by remember { mutableStateOf("") }
    var includeSecrets by remember { mutableStateOf(false) }

    val exporting = mode == BackupPassphraseMode.Export
    // Exporting: a passphrase is optional unless passwords are included, but
    // once typed it must be confirmed — a typo produces a file nobody can open.
    // Restoring: the passphrase is checked against the file immediately, so
    // there is nothing to confirm.
    val mismatched = exporting && passphrase != confirmation
    val missing = if (exporting) includeSecrets && passphrase.isEmpty() else passphrase.isEmpty()

    AlertDialog(
        onDismissRequest = { if (!busy) onDismiss() },
        title = { Text(if (exporting) tr("settings.backup.exportTitle") else tr("settings.backup.restoreTitle")) },
        text = {
            Column(
                modifier = Modifier.verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                if (exporting) {
                    Row(
                        Modifier.fillMaxWidth().clickable(enabled = !busy) { includeSecrets = !includeSecrets },
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Checkbox(checked = includeSecrets, onCheckedChange = { includeSecrets = it }, enabled = !busy)
                        Column(Modifier.weight(1f)) {
                            Text(tr("settings.backup.includeSecrets"), style = MaterialTheme.typography.bodyMedium)
                            Text(
                                tr("settings.backup.includeSecretsHint"),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
                OutlinedTextField(
                    value = passphrase,
                    onValueChange = { passphrase = it },
                    label = { Text(tr("settings.backup.passphrase")) },
                    singleLine = true,
                    enabled = !busy,
                    visualTransformation = PasswordVisualTransformation(),
                    modifier = Modifier.fillMaxWidth(),
                )
                if (exporting) {
                    OutlinedTextField(
                        value = confirmation,
                        onValueChange = { confirmation = it },
                        label = { Text(tr("settings.backup.passphraseConfirm")) },
                        singleLine = true,
                        enabled = !busy,
                        isError = mismatched && confirmation.isNotEmpty(),
                        visualTransformation = PasswordVisualTransformation(),
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
                Text(
                    if (exporting) tr("settings.backup.passphraseHint") else tr("settings.backup.restoreHint"),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (mismatched && confirmation.isNotEmpty()) {
                    Text(
                        tr("settings.backup.passphraseMismatch"),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
                if (error.isNotEmpty()) {
                    Text(error, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onConfirm(passphrase, exporting && includeSecrets) },
                enabled = !busy && !missing && !mismatched,
            ) {
                Text(if (exporting) tr("common.export") else tr("settings.backup.restoreAction"))
            }
        },
        dismissButton = { TextButton(onClick = onDismiss, enabled = !busy) { Text(tr("buttons.cancel")) } },
    )
}
