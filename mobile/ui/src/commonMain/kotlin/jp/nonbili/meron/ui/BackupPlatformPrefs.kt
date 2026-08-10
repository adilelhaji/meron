package jp.nonbili.meron.ui

// Mobile settings that belong in a backup but are invisible to the core.
//
// Desktop keeps every setting as a row in the core `settings` table, so its
// backup captures them for free. Mobile instead holds appearance, language,
// layout, kanban boards and friends in platform storage (SharedPreferences /
// NSUserDefaults), which the Rust core cannot reach. These are collected here,
// sent along with the export, and written back on restore — inside the same
// (optionally encrypted) document as everything else.
//
// The list is curated rather than a dump of the whole store: session and
// in-flight state (last opened folder, pending OAuth handshakes, kanban search
// text) would be actively wrong to carry to another device.

/** Which preference store a key lives in. Namespaced so the two cannot collide. */
private enum class PrefStore(
    val prefix: String,
) {
    App("app"),
    Kanban("kanban"),
}

private enum class PrefType { Str, Bool, Int, StrSet }

private data class BackedUpPref(
    val store: PrefStore,
    val key: String,
    val type: PrefType,
) {
    /** Wire key, e.g. `app:appearance_mode_v1`. */
    val wireKey: String get() = "${store.prefix}:$key"
}

private val backedUpPrefs =
    listOf(
        // Appearance and language.
        BackedUpPref(PrefStore.App, APPEARANCE_MODE_PREF, PrefType.Str),
        BackedUpPref(PrefStore.App, APP_LANGUAGE_PREF, PrefType.Str),
        BackedUpPref(PrefStore.App, MESSAGE_FONT_SCALE_PREF, PrefType.Int),
        BackedUpPref(PrefStore.App, SHOW_SENDER_IMAGES_PREF, PrefType.Bool),
        BackedUpPref(PrefStore.App, SHOW_UNREAD_BADGES_PREF, PrefType.Bool),
        // Layout and navigation.
        BackedUpPref(PrefStore.App, SHOW_UNIFIED_INBOX_PREF, PrefType.Bool),
        BackedUpPref(PrefStore.App, CONVERSATION_LAYOUT_PREF, PrefType.Str),
        BackedUpPref(PrefStore.App, SEND_SHORTCUT_PREF, PrefType.Str),
        BackedUpPref(PrefStore.App, HIDDEN_NAV_ACCOUNTS_PREF, PrefType.StrSet),
        BackedUpPref(PrefStore.App, KANBAN_COLUMN_WIDTH_PREF, PrefType.Int),
        // Sync and notifications.
        BackedUpPref(PrefStore.App, LIVE_MAIL_PUSH_PREF, PrefType.Bool),
        BackedUpPref(PrefStore.App, BACKGROUND_SYNC_ENABLED_PREF, PrefType.Bool),
        BackedUpPref(PrefStore.App, POLL_INTERVAL_MINUTES_PREF, PrefType.Int),
        // Kanban boards, in their own store.
        BackedUpPref(PrefStore.Kanban, KANBAN_BOARDS_PREF, PrefType.Str),
        BackedUpPref(PrefStore.Kanban, ACTIVE_KANBAN_BOARD_PREF, PrefType.Str),
    )

// Two arbitrary, distinct probe defaults. `AppPreferences` has no "contains"
// call, so a key is read twice, asking for a different default each time: the
// answers agree only when a real value is stored, and differ when both reads
// fell through to the default. That separates "unset" from "set to the type
// default" without reserving a sentinel the user might legitimately hold —
// backing up an explicit `false` as if it were "never touched" would silently
// switch settings back on when restored.
private const val PROBE_A = "meron-probe-a"
private const val PROBE_B = "meron-probe-b"

/**
 * Read the backed-up preferences as JSON-ready values. Keys the user has never
 * set are omitted, so a restore does not write defaults over the target's own
 * choices.
 */
internal fun collectPlatformPrefs(
    app: AppPreferences,
    kanban: AppPreferences,
): Map<String, Any> {
    val out = LinkedHashMap<String, Any>()
    for (pref in backedUpPrefs) {
        val prefs = if (pref.store == PrefStore.Kanban) kanban else app
        when (pref.type) {
            PrefType.Str -> {
                val value = prefs.getString(pref.key, PROBE_A)
                if (value == prefs.getString(pref.key, PROBE_B)) out[pref.wireKey] = value
            }

            PrefType.StrSet -> {
                val value = prefs.getStringSet(pref.key, setOf(PROBE_A))
                if (value == prefs.getStringSet(pref.key, setOf(PROBE_B))) {
                    out[pref.wireKey] = value.toList()
                }
            }

            PrefType.Bool -> {
                val value = prefs.getBoolean(pref.key, false)
                if (value == prefs.getBoolean(pref.key, true)) out[pref.wireKey] = value
            }

            PrefType.Int -> {
                val value = prefs.getInt(pref.key, Int.MIN_VALUE)
                if (value == prefs.getInt(pref.key, Int.MAX_VALUE)) out[pref.wireKey] = value
            }
        }
    }
    return out
}

/**
 * Write restored preferences back into platform storage, ignoring anything the
 * backup carried that this build does not recognise (a newer version's keys) or
 * whose type does not match. Returns how many were applied.
 */
internal fun applyPlatformPrefs(
    app: AppPreferences,
    kanban: AppPreferences,
    values: Map<String, Any?>,
): Int {
    var applied = 0
    for (pref in backedUpPrefs) {
        val value = values[pref.wireKey] ?: continue
        val prefs = if (pref.store == PrefStore.Kanban) kanban else app
        val wrote =
            when (pref.type) {
                PrefType.Str -> {
                    (value as? String)?.also { prefs.putString(pref.key, it) } != null
                }

                PrefType.Bool -> {
                    (value as? Boolean)?.also { prefs.putBoolean(pref.key, it) } != null
                }

                // JSON numbers may arrive as any numeric type.
                PrefType.Int -> {
                    (value as? Number)?.also { prefs.putInt(pref.key, it.toInt()) } != null
                }

                PrefType.StrSet -> {
                    (value as? Collection<*>)
                        ?.filterIsInstance<String>()
                        ?.also { prefs.putStringSet(pref.key, it.toSet()) } != null
                }
            }
        if (wrote) applied += 1
    }
    return applied
}
