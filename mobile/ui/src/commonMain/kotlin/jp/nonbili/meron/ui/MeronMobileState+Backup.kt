package jp.nonbili.meron.ui

import jp.nonbili.meron.shared.ExportBackupParams
import jp.nonbili.meron.shared.ImportBackupParams
import jp.nonbili.meron.shared.MobileMailCommandClient
import jp.nonbili.meron.shared.encodeBackupPlatformPrefs
import jp.nonbili.meron.shared.parseBackupExportResponse
import jp.nonbili.meron.shared.parseBackupImportResponse
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// Backup / restore of the app's configuration: accounts and their connection
// settings, per-account prefs, RSS subscriptions and the settings table. Cached
// mail is not included — it re-syncs from the server — so the file stays small
// and moves between phone and desktop.
//
// The passphrase flow has two entry points because encryption is discovered at
// different times on each side: exporting asks up front (the user chooses),
// restoring asks only once the chosen file turns out to be encrypted.

/** Which half of the flow the passphrase sheet is collecting for. */
internal enum class BackupPassphraseMode { Export, Restore }

/**
 * Catalog lookup outside a composition. `tr` needs `LocalAppLocale`, which these
 * state functions have no access to, so resolve the tag from prefs the same way
 * the root composable does.
 */
internal fun MeronMobileState.trs(
    key: String,
    args: Map<String, Any?> = emptyMap(),
): String = localizedString(loadAppLanguageTag(prefs).ifBlank { "en" }, key, args)

/**
 * Serialize a backup and hand it to the platform save-file picker.
 *
 * `passphrase` encrypts the document; `includeSecrets` additionally embeds
 * account passwords and OAuth tokens, which the core refuses without one.
 */
internal fun MeronMobileState.exportBackup(
    includeSecrets: Boolean,
    passphrase: String,
) {
    if (!coreLoaded) {
        status = coreUnavailableMessage
        return
    }
    backupBusy = true
    scope.launch {
        runCatching {
            withContext(ioDispatcher) {
                MobileMailCommandClient(core).exportBackup(
                    ExportBackupParams(
                        includeSecrets = includeSecrets,
                        passphrase = passphrase,
                        // Appearance, language, layout and kanban boards live in
                        // platform storage the core cannot read, so send them
                        // along to be embedded in the same document.
                        platformJson =
                            encodeBackupPlatformPrefs(collectPlatformPrefs(prefs, kanbanPrefs)),
                    ),
                )
            }
        }.onSuccess { response ->
            val document = parseBackupExportResponse(response)
            if (document.isBlank()) {
                status = trs("settings.backup.exportFailed")
            } else {
                backupPassphraseMode = null
                pendingBackupExport = document
                launchBackupExport(backupFileName())
                status = trs("settings.backup.exported")
            }
        }.onFailure {
            status = "${trs("settings.backup.exportFailed")}: ${it.message}"
        }
        backupBusy = false
    }
}

/**
 * Restore from a file the user picked.
 *
 * An encrypted file comes back as `needsPassphrase` rather than an error: the
 * document is kept in [MeronMobileState.pendingBackupRestore] so the retry
 * decrypts it without making the user find the file again.
 */
internal fun MeronMobileState.importBackup(
    document: String,
    passphrase: String = "",
) {
    if (!coreLoaded) {
        status = coreUnavailableMessage
        return
    }
    if (document.isBlank()) {
        status = trs("settings.backup.restoreFailed")
        return
    }
    backupBusy = true
    scope.launch {
        runCatching {
            withContext(ioDispatcher) {
                MobileMailCommandClient(core).importBackup(
                    ImportBackupParams(backup = document, passphrase = passphrase),
                )
            }
        }.onSuccess { response ->
            val result = parseBackupImportResponse(response)
            if (result.needsPassphrase) {
                // Keep the document so the retry doesn't re-open the picker.
                pendingBackupRestore = document
                backupPassphraseError = ""
                backupPassphraseMode = BackupPassphraseMode.Restore
            } else {
                closeBackupPassphrase()
                // Preferences the core carried but cannot write: appearance,
                // language and the rest are ours to put back.
                val restoredPrefs = applyPlatformPrefs(prefs, kanbanPrefs, result.platform)
                status =
                    when {
                        // Appearance and language are read into Compose state at
                        // startup, so they only take effect on the next launch;
                        // say so rather than looking like nothing happened.
                        restoredPrefs > 0 -> {
                            trs("settings.backup.restoredRestart")
                        }

                        result.accounts == 0 && result.skipped > 0 -> {
                            trs("settings.backup.restoredNothingNew")
                        }

                        else -> {
                            trs("settings.backup.restored", mapOf("count" to result.accounts))
                        }
                    }
                // Restored rows are in the store but not in this state: reload
                // accounts (which re-seeds selection, folders and boards) and
                // the app-wide proxy, which the socket layer reads separately.
                listAccounts()
                loadAppProxy()
            }
        }.onFailure {
            val message = it.message.orEmpty()
            // A wrong passphrase keeps the sheet open for a retype; anything
            // else is a real failure and closes it.
            if (message.contains("wrong passphrase") && backupPassphraseMode == BackupPassphraseMode.Restore) {
                backupPassphraseError = trs("settings.backup.wrongPassphrase")
            } else {
                closeBackupPassphrase()
                status = "${trs("settings.backup.restoreFailed")}: $message"
            }
        }
        backupBusy = false
    }
}

/** Restore using the passphrase just typed into the sheet. */
internal fun MeronMobileState.retryBackupRestore(passphrase: String) {
    importBackup(pendingBackupRestore, passphrase)
}

/** Dismiss the passphrase sheet and drop whatever it was working on. */
internal fun MeronMobileState.closeBackupPassphrase() {
    backupPassphraseMode = null
    backupPassphraseError = ""
    pendingBackupRestore = ""
}

/**
 * Name offered to the save-file picker. Dated so successive backups don't
 * silently overwrite each other, and `.json` because the file is plain JSON
 * whose envelope stays readable even when the payload is encrypted.
 */
internal fun backupFileName(nowMillis: Long = currentTimeMillis()): String = "meron-backup-${isoDate(nowMillis)}.json"

/**
 * `YYYY-MM-DD` in UTC for an epoch-millis instant.
 *
 * Written out rather than delegated to a platform formatter because this only
 * ever labels a filename: a fixed, locale-independent form is what's wanted,
 * and it keeps the helper in commonMain and directly testable.
 */
internal fun isoDate(nowMillis: Long): String {
    // Civil-from-days (Howard Hinnant's algorithm), shifting the era to
    // 0000-03-01 so leap days land at the end of a 400-year cycle.
    val days = floorDiv(nowMillis, 86_400_000L)
    val z = days + 719_468L
    val era = floorDiv(z, 146_097L)
    val dayOfEra = z - era * 146_097L
    val yearOfEra = (dayOfEra - dayOfEra / 1460 + dayOfEra / 36_524 - dayOfEra / 146_096) / 365
    val year = yearOfEra + era * 400
    val dayOfYear = dayOfEra - (365 * yearOfEra + yearOfEra / 4 - yearOfEra / 100)
    val shiftedMonth = (5 * dayOfYear + 2) / 153
    val day = dayOfYear - (153 * shiftedMonth + 2) / 5 + 1
    val month = if (shiftedMonth < 10) shiftedMonth + 3 else shiftedMonth - 9
    val calendarYear = if (month <= 2) year + 1 else year
    return "$calendarYear-${pad2(month)}-${pad2(day)}"
}

private fun floorDiv(
    value: Long,
    divisor: Long,
): Long {
    val quotient = value / divisor
    return if (value % divisor != 0L && (value xor divisor) < 0) quotient - 1 else quotient
}

private fun pad2(value: Long): String = if (value < 10) "0$value" else "$value"
