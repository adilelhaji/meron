package jp.nonbili.meron.shared

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class SignatureTest {
    private val account = AccountSummary(id = "acc", email = "me@example.com")

    @Test
    fun accountFollowsTheAppSignatureUntilItOverridesIt() {
        assertEquals("<p>App</p>", resolveSignatureHtml(account, "<p>App</p>"))
        assertEquals(
            "<p>Mine</p>",
            resolveSignatureHtml(account.copy(signature = SignatureSpec("custom", "<p>Mine</p>")), "<p>App</p>"),
        )
        assertEquals("", resolveSignatureHtml(account.copy(signature = SignatureSpec("none", "<p>Mine</p>")), "<p>App</p>"))
        assertEquals("<p>App</p>", resolveSignatureHtml(account.copy(signature = SignatureSpec("global", "<p>Mine</p>")), "<p>App</p>"))
    }

    @Test
    fun blankSignaturesResolveToNothing() {
        assertTrue(signatureIsBlank("<p></p>"))
        assertTrue(signatureIsBlank("<p>&nbsp;</p>"))
        assertTrue(!signatureIsBlank("<p><img src=\"/media/logo.png\"></p>"))
        assertEquals("", resolveSignatureHtml(account, "<p>&nbsp;</p>"))
    }

    @Test
    fun htmlBecomesTheLinesTheComposerEdits() {
        assertEquals("Ping\nPong", signaturePlainText("<p>Ping<br>Pong</p>"))
        assertEquals("Ping\n\nPong", signaturePlainText("<p>Ping</p><p></p><p>Pong</p>"))
        assertEquals("R&D <ping>", signaturePlainText("<p>R&amp;D &lt;ping&gt;</p>"))
        assertEquals("", signaturePlainText(""))
    }

    @Test
    fun signatureSitsBelowTypedTextAndAboveAQuote() {
        assertEquals("\n\nPing", bodyWithSignature("", "Ping"))
        assertEquals("typed\n\nPing", bodyWithSignature("typed", "Ping"))
        assertEquals(
            "\n\nPing\n\n> quoted",
            bodyWithSignature("> quoted", "Ping", SignaturePlacement.AboveQuote),
        )
        assertEquals("typed", bodyWithSignature("typed", ""))
    }

    @Test
    fun accountSignatureParamsClearTheOverrideWithNull() {
        assertEquals(
            """{"id":"acc","signature":null}""",
            AccountSignatureParams("acc", null).toJson(),
        )
        assertEquals(
            """{"id":"acc","signature":{"mode":"custom","html":"<p>Ping</p>"}}""",
            AccountSignatureParams("acc", SignatureSpec("custom", "<p>Ping</p>")).toJson(),
        )
    }

    @Test
    fun accountListCarriesTheSignatureOverride() {
        val accounts =
            parseAccountListResponse(
                """{"accounts":[{"id":"acc","email":"me@example.com","signature":{"mode":"custom","html":"<p>Ping</p>"}}]}""",
            )
        assertEquals(SignatureSpec("custom", "<p>Ping</p>"), accounts.single().signature)

        val plain = parseAccountListResponse("""{"accounts":[{"id":"acc","email":"me@example.com"}]}""")
        assertEquals(SignatureSpec.followApp, plain.single().signature)
    }
}
