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
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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
