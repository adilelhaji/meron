package jp.nonbili.meron.ui

import jp.nonbili.meron.shared.AccountSummary
import jp.nonbili.meron.shared.CertificateProtocol
import jp.nonbili.meron.shared.SendMailParams
import jp.nonbili.meron.shared.ServerCertificate
import kotlinx.coroutines.CancellationException

internal data class MobileSyncError(
    val accountId: String?,
    val message: String,
)

/**
 * A server certificate the user is being asked to trust, and the account and
 * server it belongs to. [protocol] decides which pin an acceptance writes.
 */
internal data class MobileCertPrompt(
    val accountId: String,
    val host: String,
    val port: Int,
    val protocol: CertificateProtocol,
    val certificate: ServerCertificate,
)

/** A prepared message whose send failed and can be retried as-is. */
internal data class PendingComposeSend(
    val accountId: String,
    val params: SendMailParams,
)

internal class AccountSyncException(
    val accountId: String,
    cause: Throwable,
) : Exception(cause.message, cause)

internal suspend fun <T> withSyncAccountContext(
    accountId: String,
    block: suspend () -> T,
): T =
    try {
        block()
    } catch (failure: CancellationException) {
        throw failure
    } catch (failure: Throwable) {
        throw AccountSyncException(accountId, failure)
    }

internal fun mobileConnectivityAccountLabel(
    accountId: String?,
    accounts: List<AccountSummary>,
): String? {
    if (accountId.isNullOrBlank()) return null
    val account = accounts.firstOrNull { it.id == accountId } ?: return accountId
    val displayName = account.displayName.trim()
    val email = account.email.trim()
    if (displayName.isBlank()) return email.ifBlank { account.id }

    val duplicateName =
        accounts.any { candidate ->
            candidate.id != account.id &&
                candidate.displayName.trim().equals(displayName, ignoreCase = true)
        }
    return if (duplicateName && email.isNotBlank() && !email.equals(displayName, ignoreCase = true)) {
        "$displayName ($email)"
    } else {
        displayName
    }
}

internal fun mobileProxyEndpointFromSyncError(error: String): String? {
    val endpoint = """([^\s]+:\d+)"""
    return Regex("""\bconnect to proxy\s+$endpoint""", RegexOption.IGNORE_CASE).find(error)?.groupValues?.get(1)
        ?: Regex("""\b(?:http|socks5) proxy\s+$endpoint""", RegexOption.IGNORE_CASE).find(error)?.groupValues?.get(1)
}
