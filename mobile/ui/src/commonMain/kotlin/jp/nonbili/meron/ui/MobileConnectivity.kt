package jp.nonbili.meron.ui

import jp.nonbili.meron.shared.AccountSummary
import kotlinx.coroutines.CancellationException

internal data class MobileSyncError(
    val accountId: String?,
    val message: String,
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
