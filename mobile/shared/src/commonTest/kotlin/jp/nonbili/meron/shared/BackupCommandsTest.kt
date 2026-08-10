package jp.nonbili.meron.shared

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class BackupCommandsTest {
    @Test
    fun exportBackupParamsSerializeSecretsFlagAndPassphrase() {
        assertEquals(
            """{"include_secrets":false,"passphrase":"","platform":{}}""",
            ExportBackupParams().toJson(),
        )
        assertEquals(
            """{"include_secrets":true,"passphrase":"correct horse","platform":{}}""",
            ExportBackupParams(includeSecrets = true, passphrase = "correct horse").toJson(),
        )
    }

    @Test
    fun exportBackupParamsEmbedThePlatformPreferences() {
        assertEquals(
            """{"include_secrets":false,"passphrase":"","platform":{"app:lang":"ja"}}""",
            ExportBackupParams(platformJson = """{"app:lang":"ja"}""").toJson(),
        )
        // A blank map must still leave the payload valid JSON.
        assertEquals(
            """{"include_secrets":false,"passphrase":"","platform":{}}""",
            ExportBackupParams(platformJson = "").toJson(),
        )
    }

    @Test
    fun platformPreferencesRoundTripThroughJson() {
        val values =
            mapOf<String, Any>(
                "app:appearance_mode_v1" to "Indigo",
                "app:message_font_scale_v1" to 115,
                "app:show_unread_badges_v1" to true,
                "app:background_sync_enabled_v1" to false,
                "app:hidden_navigation_accounts_v1" to listOf("a@example.com", "b@example.com"),
                "kanban:kanban_boards_v1" to """[{"id":"b1","name":"Work"}]""",
            )

        val encoded = encodeBackupPlatformPrefs(values)
        val decoded = parseBackupPlatformPrefs("""{"platform":$encoded}""")

        assertEquals("Indigo", decoded["app:appearance_mode_v1"])
        // JSON numbers decode as Long.
        assertEquals(115L, decoded["app:message_font_scale_v1"])
        assertEquals(true, decoded["app:show_unread_badges_v1"])
        assertEquals(false, decoded["app:background_sync_enabled_v1"])
        assertEquals(
            listOf("a@example.com", "b@example.com"),
            decoded["app:hidden_navigation_accounts_v1"],
        )
        // A string that is itself JSON must survive escaping intact.
        assertEquals("""[{"id":"b1","name":"Work"}]""", decoded["kanban:kanban_boards_v1"])
    }

    @Test
    fun platformPreferencesEncodeEmptyAsAnEmptyObject() {
        assertEquals("{}", encodeBackupPlatformPrefs(emptyMap()))
    }

    @Test
    fun parsingPlatformPreferencesToleratesAMissingOrOddPayload() {
        assertEquals(emptyMap(), parseBackupPlatformPrefs("""{"accounts":1}"""))
        assertEquals(emptyMap(), parseBackupPlatformPrefs("not json"))
        // Values the pref store cannot hold are skipped, not guessed at.
        assertEquals(
            emptyMap(),
            parseBackupPlatformPrefs("""{"platform":{"a":null,"b":{"nested":1},"c":1.5}}"""),
        )
    }

    @Test
    fun importResultCarriesPlatformPreferencesAlongsideTheCounts() {
        val result =
            parseBackupImportResponse(
                """{"accounts":1,"skipped":0,"feeds":0,"settings":3,"secrets":1,"platform":{"app:lang":"ja"}}""",
            )
        assertEquals(1, result.accounts)
        assertEquals(3, result.settings)
        assertEquals("ja", result.platform["app:lang"])
    }

    @Test
    fun importBackupParamsEscapeTheEmbeddedDocument() {
        // The backup is JSON inside a JSON string, so its quotes must escape.
        assertEquals(
            """{"backup":"{\"meron_backup\":1}","passphrase":"pw"}""",
            ImportBackupParams(backup = """{"meron_backup":1}""", passphrase = "pw").toJson(),
        )
    }

    @Test
    fun backupRequestsUseTheProtocolMethodNames() {
        assertEquals(
            """{"id":90,"method":"backup.export","params":{"include_secrets":true,"passphrase":"pw","platform":{}}}""",
            backupExportRequest(id = 90, params = ExportBackupParams(includeSecrets = true, passphrase = "pw")).toJson(),
        )
        assertEquals(
            """{"id":91,"method":"backup.import","params":{"backup":"{}","passphrase":""}}""",
            backupImportRequest(id = 91, params = ImportBackupParams(backup = "{}")).toJson(),
        )
    }

    @Test
    fun parseBackupExportResponseReadsTheDocument() {
        assertEquals(
            """{"meron_backup":1}""",
            parseBackupExportResponse("""{"backup":"{\"meron_backup\":1}"}"""),
        )
        assertEquals("", parseBackupExportResponse("""{"other":"value"}"""))
        assertEquals("", parseBackupExportResponse("not json"))
    }

    @Test
    fun parseBackupImportResponseReadsTheCounts() {
        val result =
            parseBackupImportResponse(
                """{"accounts":2,"skipped":1,"feeds":5,"settings":9,"secrets":2}""",
            )
        assertFalse(result.needsPassphrase)
        assertEquals(2, result.accounts)
        assertEquals(1, result.skipped)
        assertEquals(5, result.feeds)
        assertEquals(9, result.settings)
        assertEquals(2, result.secrets)
    }

    @Test
    fun parseBackupImportResponseFlagsAnEncryptedFile() {
        val result = parseBackupImportResponse("""{"needs_passphrase":true}""")
        assertTrue(result.needsPassphrase)
        // Nothing was restored, so the counts stay at zero.
        assertEquals(0, result.accounts)
    }

    @Test
    fun parseBackupImportResponseDefaultsMissingCountsToZero() {
        val result = parseBackupImportResponse("{}")
        assertFalse(result.needsPassphrase)
        assertEquals(0, result.accounts)
        assertEquals(0, result.skipped)
        assertEquals(0, result.feeds)
        assertEquals(0, result.settings)
        assertEquals(0, result.secrets)
    }
}
