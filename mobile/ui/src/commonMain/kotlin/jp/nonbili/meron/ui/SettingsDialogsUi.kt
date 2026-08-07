package jp.nonbili.meron.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

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
