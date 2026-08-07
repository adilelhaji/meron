package jp.nonbili.meron

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import org.json.JSONObject

/** One new mail, as the shade shows it. Built from a `mail.newMessages` entry. */
data class NewMailItem(
    val uid: Long,
    val from: String,
    val subject: String,
    val preview: String,
    val threadKey: String,
    val date: Long,
)

/** A batch of arrivals for one account: one notification per item, under a
 *  per-account group summary. */
data class NewMailBatch(
    val accountId: String,
    val accountName: String,
    val folder: String,
    val count: Int,
    val items: List<NewMailItem>,
)

object AndroidNotificationService {
    private const val CHANNEL_ID = "meron_sync"

    /** New mail lives on its own channel so muting background-refresh status
     *  notifications doesn't mute the mail itself (and vice versa). */
    private const val MAIL_CHANNEL_ID = "meron_new_mail"

    /** Private extra: the summary row a child stands for, see [activeLines]. */
    private const val EXTRA_SUMMARY_LINE = "jp.nonbili.meron.extra.SUMMARY_LINE"
    private const val NOTIFICATION_ID = 1001
    const val EXTRA_ACCOUNT_ID = "jp.nonbili.meron.extra.ACCOUNT_ID"
    const val EXTRA_FOLDER = "jp.nonbili.meron.extra.FOLDER"
    const val EXTRA_THREAD_KEY = "jp.nonbili.meron.extra.THREAD_KEY"

    fun refreshChannelIdForTesting(): String = CHANNEL_ID

    fun mailChannelIdForTesting(): String = MAIL_CHANNEL_ID

    fun ensureChannels(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                "Mail sync",
                NotificationManager.IMPORTANCE_DEFAULT,
            ).apply {
                description = "Background mail refresh status"
            },
        )
        manager.createNotificationChannel(
            NotificationChannel(
                MAIL_CHANNEL_ID,
                "New mail",
                NotificationManager.IMPORTANCE_DEFAULT,
            ).apply {
                description = "One notification per message that arrives"
            },
        )
    }

    fun canNotify(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED

    fun notifyRefreshComplete(
        context: Context,
        body: String,
    ) {
        if (!canNotify(context)) return
        ensureChannels(context)
        val notification =
            NotificationCompat
                .Builder(context, CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_stat_mail)
                .setContentTitle("Refresh complete")
                .setContentText(body)
                .setStyle(NotificationCompat.BigTextStyle().bigText(body))
                .setPriority(NotificationCompat.PRIORITY_DEFAULT)
                .setAutoCancel(true)
                .build()
        try {
            NotificationManagerCompat.from(context).notify(NOTIFICATION_ID, notification)
        } catch (_: SecurityException) {
            // Notification permission can change after canNotify() checks it.
        }
    }

    /** Posts new-mail notifications from a `mail.newMessages`-shaped detail
     *  object; no-op when the account is muted. */
    fun notifyNewMail(
        context: Context,
        detail: JSONObject,
    ) {
        if (detail.optBoolean("muted")) return
        notifyNewMail(context, parseNewMailBatch(detail))
    }

    /** One notification per arrival plus a group summary, the way the platform
     *  expects a mail client to report a batch: the summary collapses the lot
     *  under the account, and each child opens its own thread. */
    fun notifyNewMail(
        context: Context,
        batch: NewMailBatch,
    ) {
        if (batch.items.isEmpty() || !canNotify(context)) return
        ensureChannels(context)
        val manager = NotificationManagerCompat.from(context)
        val groupKey = newMailGroupKey(batch.accountId)
        // Read the shade before posting: afterwards it also holds this batch,
        // and those lines would be counted a second time.
        val carriedOver = activeLines(context, groupKey)
        try {
            for (item in batch.items) {
                manager.notify(
                    newMailNotificationId(batch.accountId, item),
                    buildNewMailChild(context, batch, item, groupKey),
                )
            }
            // Posted last so its alert is the one the user hears (the children
            // are silenced via GROUP_ALERT_SUMMARY).
            manager.notify(
                newMailSummaryId(batch.accountId),
                buildNewMailSummary(context, batch, groupKey, carriedOver),
            )
        } catch (_: SecurityException) {
            // Notification permission can change after canNotify() checks it.
        }
    }

    private fun buildNewMailChild(
        context: Context,
        batch: NewMailBatch,
        item: NewMailItem,
        groupKey: String,
    ): android.app.Notification {
        val title = newMailChildTitle(item.from, batch.accountName)
        // Lock screens that hide sensitive content show this instead: who wrote
        // and about what, but never the body.
        val publicVersion =
            NotificationCompat
                .Builder(context, MAIL_CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_stat_mail)
                .setContentTitle(title)
                .setContentText(item.subject.ifBlank { "New mail arrived" })
                .setGroup(groupKey)
                .build()
        return NotificationCompat
            .Builder(context, MAIL_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_mail)
            .setContentTitle(title)
            .setContentText(newMailChildText(item.subject, item.preview))
            .setStyle(
                NotificationCompat
                    .BigTextStyle()
                    .bigText(newMailChildBigText(item.subject, item.preview))
                    .setSummaryText(batch.accountName),
            ).setContentIntent(openAppIntent(context, batch.accountId, batch.folder, item.threadKey))
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(publicVersion)
            .setGroup(groupKey)
            .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_SUMMARY)
            // The summary row this child contributes, carried on the child so a
            // later batch can read it back verbatim: the visible text is
            // "subject - preview", which would not match a freshly built line.
            .addExtras(
                android.os.Bundle().apply {
                    putString(EXTRA_SUMMARY_LINE, newMailInboxLine(item.from, item.subject))
                },
            ).apply { if (item.date > 0) setWhen(item.date * 1000L) }
            .setAutoCancel(true)
            .build()
    }

    private fun buildNewMailSummary(
        context: Context,
        batch: NewMailBatch,
        groupKey: String,
        carriedOver: List<String>,
    ): android.app.Notification {
        // Lines the user hasn't dismissed yet (read back from the shade) plus
        // this batch's, so the summary describes what is actually showing rather
        // than only the newest arrivals.
        val lines = (batch.items.map { newMailInboxLine(it.from, it.subject) } + carriedOver).distinct()
        val style = NotificationCompat.InboxStyle().setSummaryText(batch.accountName)
        lines.take(NEW_MAIL_SUMMARY_LINES).forEach { style.addLine(it) }
        return NotificationCompat
            .Builder(context, MAIL_CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_mail)
            .setContentTitle(batch.accountName.ifBlank { "New mail" })
            // `count` can exceed the listed messages when a batch is larger than
            // the detail carries; never report fewer than the shade is showing.
            .setContentText(newMailSummaryText(maxOf(batch.count, lines.size)))
            .setStyle(style)
            .setContentIntent(openAppIntent(context, batch.accountId, batch.folder))
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .setGroup(groupKey)
            .setGroupSummary(true)
            .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_SUMMARY)
            .setAutoCancel(true)
            .build()
    }

    /** Inbox lines for the group's notifications that are still showing, newest
     *  first. Read from the shade rather than remembered in-process, because a
     *  background sync posts from a worker that doesn't outlive the batch. */
    private fun activeLines(
        context: Context,
        groupKey: String,
    ): List<String> =
        try {
            context
                .getSystemService(NotificationManager::class.java)
                .activeNotifications
                .asSequence()
                .map { it.notification }
                .filter { it.group == groupKey && (it.flags and android.app.Notification.FLAG_GROUP_SUMMARY) == 0 }
                .sortedByDescending { it.`when` }
                .mapNotNull { notification ->
                    val extras = notification.extras ?: return@mapNotNull null
                    val line =
                        extras.getString(EXTRA_SUMMARY_LINE) ?: run {
                            val title = extras.getCharSequence(NotificationCompat.EXTRA_TITLE)?.toString().orEmpty()
                            val text = extras.getCharSequence(NotificationCompat.EXTRA_TEXT)?.toString().orEmpty()
                            newMailInboxLine(title, text)
                        }
                    line.ifBlank { null }
                }.toList()
        } catch (_: RuntimeException) {
            // Reading the shade back is an enhancement, not the notification
            // itself: on any refusal fall back to this batch's lines alone.
            emptyList()
        }

    fun openAppIntent(
        context: Context,
        accountId: String = "",
        folder: String = "",
        threadKey: String = "",
    ): PendingIntent {
        val intent =
            Intent(context, ComposeMainActivity::class.java).apply {
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP
                // A blank thread key still navigates: the group summary targets
                // the account's folder, and only the thread is left unopened.
                if (accountId.isNotBlank() && folder.isNotBlank()) {
                    putExtra(EXTRA_ACCOUNT_ID, accountId)
                    putExtra(EXTRA_FOLDER, folder)
                    putExtra(EXTRA_THREAD_KEY, threadKey)
                }
            }
        return PendingIntent.getActivity(
            context,
            listOf(accountId, folder, threadKey).joinToString("|").hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }
}

/** Most lines an expanded group summary lists; beyond this the shade elides
 *  them anyway and `count` in the collapsed line carries the total. */
const val NEW_MAIL_SUMMARY_LINES = 6

/** Read a `mail.newMessages` detail into a batch.
 *
 *  `messages` carries one entry per arrival. RSS arrivals — and any core older
 *  than the per-message payload — send only the top-level summary fields, which
 *  degrade to a single notification for the batch. */
fun parseNewMailBatch(detail: JSONObject): NewMailBatch {
    val accountId = detail.optString("account")
    val listed = detail.optJSONArray("messages")
    val items =
        if (listed != null && listed.length() > 0) {
            (0 until listed.length()).mapNotNull { index ->
                listed.optJSONObject(index)?.let { entry ->
                    NewMailItem(
                        uid = entry.optLong("uid"),
                        from = entry.optString("from"),
                        subject = entry.optString("subject"),
                        preview = entry.optString("preview"),
                        threadKey = entry.optString("threadKey"),
                        date = entry.optLong("date"),
                    )
                }
            }
        } else {
            listOf(
                NewMailItem(
                    uid = 0,
                    from = detail.optString("from"),
                    subject = detail.optString("subject"),
                    preview = detail.optString("preview"),
                    threadKey = detail.optString("threadKey"),
                    date = 0,
                ),
            )
        }
    return NewMailBatch(
        accountId = accountId,
        accountName = detail.optString("accountName"),
        folder = detail.optString("folder"),
        count = detail.optInt("count", items.size),
        items = items,
    )
}

/** Notifications for one account share a group, so the shade collapses that
 *  account's arrivals under a single summary instead of interleaving accounts. */
fun newMailGroupKey(accountId: String): String = "jp.nonbili.meron.NEW_MAIL:$accountId"

/** Stable per-message id: re-posting the same mail (a retried sync, a push and
 *  a periodic refresh racing) updates its notification instead of stacking a
 *  duplicate. Falls back to the thread key for payloads without a UID. */
fun newMailNotificationId(
    accountId: String,
    item: NewMailItem,
): Int =
    if (item.uid > 0) {
        "$accountId#uid:${item.uid}".hashCode()
    } else {
        "$accountId#thread:${item.threadKey}#${item.subject}".hashCode()
    }

fun newMailSummaryId(accountId: String): Int = "$accountId#summary".hashCode()

/** Sender, or the account when the envelope carried no From at all. */
fun newMailChildTitle(
    from: String,
    accountName: String,
): String = from.trim().ifBlank { accountName.trim().ifBlank { "New mail" } }

/** Collapsed line: subject and the start of the body, the way the shade shows a
 *  message before it is expanded. */
fun newMailChildText(
    subject: String,
    preview: String,
): String {
    val parts = listOf(subject, preview).map { it.trim() }.filter { it.isNotEmpty() }
    return parts.joinToString(" - ").ifBlank { "New mail arrived" }
}

/** Expanded body: the subject on its own line above the body snippet, so the
 *  mail is readable without opening the app. */
fun newMailChildBigText(
    subject: String,
    preview: String,
): String {
    val parts = listOf(subject, preview).map { it.trim() }.filter { it.isNotEmpty() }
    return parts.joinToString("\n").ifBlank { "New mail arrived" }
}

/** One summary row: who wrote, and about what. */
fun newMailInboxLine(
    from: String,
    subject: String,
): String {
    val parts = listOf(from, subject).map { it.trim() }.filter { it.isNotEmpty() }
    return parts.joinToString(" - ")
}

fun newMailSummaryText(count: Int): String = if (count == 1) "1 new message" else "$count new messages"
