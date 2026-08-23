package jp.nonbili.meron.ui

import jp.nonbili.meron.shared.AccountSummary
import jp.nonbili.meron.shared.CertificateProtocol
import jp.nonbili.meron.shared.CloseableHandle
import jp.nonbili.meron.shared.CoreEvent
import jp.nonbili.meron.shared.CoreEventStream
import jp.nonbili.meron.shared.MeronCore
import jp.nonbili.meron.shared.MobileCommand
import jp.nonbili.meron.shared.ProxySpec
import jp.nonbili.meron.shared.isUntrustedCertificateError
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * A message refused by an untrusted submission certificate has to be *sent*
 * once the user trusts it — falling back to a sync would save the pin and leave
 * the message sitting unsent.
 */
class CertificateTrustFlowTest {
    @Test
    fun trustingTheCertificateSendsTheMessageThatWasRefused() =
        runBlocking {
            val core = TrustCore()
            val state = state(core, this)
            state.openCompose()
            state.to = "you@example.com"
            state.subject = "Ping"
            state.body = "Body${state.body}"

            state.sendMail()
            waitUntil { state.errorBanner != null }
            assertEquals(1, core.sendPayloads.size)
            assertTrue(isUntrustedCertificateError(assertNotNull(state.errorBanner)))

            state.showServerCertificate("a", assertNotNull(state.errorBanner))
            waitUntil { state.certPrompt != null }
            val prompt = assertNotNull(state.certPrompt)
            assertEquals(CertificateProtocol.SMTP, prompt.protocol)
            assertEquals(1025, prompt.port)
            val probe = assertNotNull(core.probePayload)
            assertTrue(probe.contains("\"protocol\":\"smtp\""), probe)
            // Probed over the account's own proxy, like the send that failed.
            assertTrue(probe.contains("\"mode\":\"socks5\""), probe)

            core.sendFails = false
            state.trustPromptedCertificate()
            waitUntil { core.sendPayloads.size == 2 }
            val pin = assertNotNull(core.pinPayload)
            assertTrue(pin.contains("\"smtp_cert_pin\":\"6f69d6a7\""), pin)
            assertTrue(!pin.contains("\"cert_pin\":"), pin)
            // The same message, Message-ID included — not whatever the composer
            // holds by now.
            assertEquals(core.sendPayloads[0], core.sendPayloads[1])
            waitUntil { state.certPrompt == null && state.errorBanner == null }
            assertNull(state.pendingCertificateRetry)
        }

    @Test
    fun anOrdinarySendFailureLeavesNothingToResume() =
        runBlocking {
            val core = TrustCore(failure = "smtp auth: authentication failed")
            val state = state(core, this)
            state.openCompose()
            state.to = "you@example.com"
            state.subject = "Ping"
            state.body = "Body${state.body}"

            state.sendMail()
            waitUntil { state.errorBanner != null }

            assertNull(state.pendingCertificateRetry)
        }

    private suspend fun waitUntil(condition: () -> Boolean) {
        withTimeout(5_000) {
            while (!condition()) delay(5)
        }
    }

    private fun state(
        core: MeronCore,
        scope: CoroutineScope,
    ): MeronMobileState =
        MeronMobileState(
            scope = scope,
            core = core,
            coreLoaded = true,
            prefs = MemoryPreferences(),
            kanbanPrefs = MemoryPreferences(),
            services = NoopPlatformServices(),
            locale = NoopLocaleController(),
            mobileHost = DefaultMobileHost(),
            settingsMirror = SettingsMirror(core, MemoryPreferences()) { true },
        ).apply {
            coreAccounts =
                listOf(
                    AccountSummary(
                        id = "a",
                        email = "a@example.com",
                        imapHost = "127.0.0.1",
                        imapPort = 1143,
                        smtpHost = "127.0.0.1",
                        smtpPort = 1025,
                        tls = false,
                        starttls = true,
                        smtpTls = false,
                        smtpStarttls = true,
                        proxy = ProxySpec("socks5", "127.0.0.1", 9050),
                    ),
                )
            selectedCoreAccountId = "a"
            appSignatureLoaded = true
        }

    /** Refuses the first send the way a bridge with a self-signed certificate does. */
    private class TrustCore(
        private val failure: String =
            "smtp-untrusted-certificate: invalid peer certificate: Other(OtherError(CaUsedAsEndEntity))",
    ) : MeronCore {
        val sendPayloads = mutableListOf<String>()
        var probePayload: String? = null
        var pinPayload: String? = null
        var sendFails = true

        override suspend fun invoke(
            command: String,
            payloadJson: String,
        ): String =
            when (command) {
                MobileCommand.AllocateIdentity -> {
                    """{"message_id":"draft-1@example.com"}"""
                }

                MobileCommand.Send -> {
                    sendPayloads += payloadJson
                    if (sendFails) throw RuntimeException(failure)
                    "{}"
                }

                MobileCommand.AccountProbeCert -> {
                    probePayload = payloadJson
                    """
                    {"certificate":{"fingerprint":"6f69d6a7","subject":"CN=127.0.0.1, O=Proton Mail Bridge",
                    "issuer":"CN=127.0.0.1, O=Proton Mail Bridge","not_before":"Sat, 22 Aug 2026 23:57:04 +0000",
                    "not_after":"Mon, 24 Aug 2026 23:57:04 +0000","self_signed":true}}
                    """.trimIndent()
                }

                MobileCommand.AccountSetCertPin -> {
                    pinPayload = payloadJson
                    """{"ok":true}"""
                }

                else -> {
                    "{}"
                }
            }

        override fun events(): CoreEventStream =
            object : CoreEventStream {
                override fun subscribe(listener: (CoreEvent) -> Unit): CloseableHandle = CloseableHandle {}
            }

        override suspend fun protocolVersion(): Int = 0
    }

    private class MemoryPreferences : AppPreferences {
        private val values = mutableMapOf<String, String>()

        override fun getString(
            key: String,
            default: String,
        ): String = values[key] ?: default

        override fun putString(
            key: String,
            value: String,
        ) {
            values[key] = value
        }

        override fun getBoolean(
            key: String,
            default: Boolean,
        ): Boolean = default

        override fun putBoolean(
            key: String,
            value: Boolean,
        ) {}

        override fun getInt(
            key: String,
            default: Int,
        ): Int = default

        override fun putInt(
            key: String,
            value: Int,
        ) {}

        override fun getStringSet(
            key: String,
            default: Set<String>,
        ): Set<String> = default

        override fun putStringSet(
            key: String,
            value: Set<String>,
        ) {}

        override fun remove(key: String) {
            values.remove(key)
        }
    }

    private class NoopPlatformServices : PlatformServices {
        override fun openUrl(url: String) {}

        override fun openOAuthUrl(
            url: String,
            callbackScheme: String,
            onCallback: (String) -> Unit,
            onFailure: (String) -> Unit,
        ) {}

        override fun copyText(
            label: String,
            value: String,
        ) {}

        override fun copyImage(
            bytes: ByteArray,
            mimeType: String,
            label: String,
        ) {}

        override fun shareFile(
            bytes: ByteArray,
            fileName: String,
            mimeType: String,
        ) {}

        override fun saveFile(
            bytes: ByteArray,
            fileName: String,
            mimeType: String,
        ) {}

        override fun pickFile(
            mimeTypes: List<String>,
            onPicked: (PickedFile?) -> Unit,
        ) {}

        override fun pickImage(onPicked: (PickedFile?) -> Unit) {}
    }

    private class NoopLocaleController : LocaleController {
        override fun systemLanguageTag(): String = ""

        override fun applySystem(tag: String) {}

        override fun deviceLanguageTag(): String = "en-US"

        override fun displayName(tag: String): String = tag
    }
}
