package jp.nonbili.meron.shared

// Signatures are stored as HTML — one app-wide signature in the core `settings`
// table, plus an optional per-account override — and are inserted into the draft
// body when a composer opens, so what the user sees is what goes out.
//
// The mobile composer edits plain text, so a signature reaches it through
// [signaturePlainText]. Rich markup written on desktop survives untouched as
// long as the text is not edited here (see the settings screen).

/** Where a signature lands relative to whatever the draft was seeded with. */
enum class SignaturePlacement {
    /** Above a seeded quote, so a forward keeps the quote last. */
    AboveQuote,

    /** Below text the user already typed (a quick reply carried into compose). */
    BelowText,
}

/**
 * The signature an account actually sends: its own override, nothing when it
 * opts out, or the app-wide signature. Blank signatures resolve to "".
 */
fun resolveSignatureHtml(
    account: AccountSummary?,
    appSignatureHtml: String,
): String {
    val signature = account?.signature ?: SignatureSpec.followApp
    val html =
        when (signature.mode) {
            "none" -> ""
            "custom" -> signature.html
            else -> appSignatureHtml
        }
    return if (signatureIsBlank(html)) "" else html
}

/** Whether signature HTML carries any visible content (text or an image). */
fun signatureIsBlank(html: String): Boolean {
    if (html.isBlank()) return true
    val stripped =
        html
            .replace(Regex("<(img|hr|br)\\b[^>]*>", RegexOption.IGNORE_CASE), "x")
            .replace(Regex("<[^>]*>"), "")
            .replace("&nbsp;", " ", ignoreCase = true)
    return stripped.isBlank()
}

private val BLOCK_TAG = Regex("</(p|div|li|tr|h[1-6]|blockquote|pre)>", RegexOption.IGNORE_CASE)
private val LINE_BREAK = Regex("<(br|hr)\\s*/?>", RegexOption.IGNORE_CASE)

/**
 * A signature's plaintext form: block elements end a line, `<br>` breaks one,
 * and the remaining markup and entities are unwrapped. Deliberately simple —
 * signatures are short, and anything richer belongs in the HTML alternative
 * the desktop composer sends.
 */
fun signaturePlainText(html: String): String {
    if (html.isBlank()) return ""
    return html
        .replace(BLOCK_TAG, "\n")
        .replace(LINE_BREAK, "\n")
        .replace(Regex("<[^>]*>"), "")
        .replace("&nbsp;", " ", ignoreCase = true)
        .replace("&lt;", "<", ignoreCase = true)
        .replace("&gt;", ">", ignoreCase = true)
        .replace("&quot;", "\"", ignoreCase = true)
        .replace("&#39;", "'")
        .replace("&amp;", "&", ignoreCase = true)
        .lines()
        .joinToString("\n") { it.trim() }
        .trim('\n')
        .trim()
}

/**
 * Place a signature in a plaintext draft body, leaving a blank line above it for
 * the cursor. Returns [body] unchanged when there is no signature.
 */
fun bodyWithSignature(
    body: String,
    signatureText: String,
    placement: SignaturePlacement = SignaturePlacement.BelowText,
): String {
    if (signatureText.isBlank()) return body
    val seeded = body.trim()
    if (seeded.isEmpty()) return "\n\n$signatureText"
    return when (placement) {
        SignaturePlacement.AboveQuote -> "\n\n$signatureText\n\n$seeded"
        SignaturePlacement.BelowText -> "$seeded\n\n$signatureText"
    }
}
