package jp.nonbili.meron.ui

import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration

internal data class InlineMessageLink(
    val start: Int,
    val end: Int,
    val url: String,
)

internal data class ParsedInlineMessageText(
    val text: String,
    val links: List<InlineMessageLink>,
)

private val inlineMessageToken =
    Regex("""(\[[^\]]+]\([^)]+\)|(?:https?://|www\.)[^\s<>"']+)""", RegexOption.IGNORE_CASE)
private val markdownMessageLink = Regex("""^\[([^\]]+)]\(([^)]+)\)$""")
private val schemelessMessageUrl = Regex("""^[\w.-]+\.[a-z]{2,}(/|$)""", RegexOption.IGNORE_CASE)

internal fun parseInlineMessageText(text: String): ParsedInlineMessageText {
    if (text.isEmpty()) return ParsedInlineMessageText("", emptyList())

    val rendered = StringBuilder()
    val links = mutableListOf<InlineMessageLink>()
    var sourceOffset = 0
    inlineMessageToken.findAll(text).forEach { match ->
        rendered.append(text, sourceOffset, match.range.first)
        val token = match.value
        val markdown = markdownMessageLink.matchEntire(token)
        val label = markdown?.groupValues?.get(1) ?: token
        val rawUrl = markdown?.groupValues?.get(2) ?: token
        val url = normalizedInlineMessageUrl(rawUrl)
        if (url == null) {
            rendered.append(token)
            sourceOffset = match.range.last + 1
            return@forEach
        }
        val start = rendered.length
        rendered.append(label)
        links += InlineMessageLink(start, rendered.length, url)
        sourceOffset = match.range.last + 1
    }
    rendered.append(text, sourceOffset, text.length)
    return ParsedInlineMessageText(rendered.toString(), links)
}

private fun normalizedInlineMessageUrl(rawUrl: String): String? =
    when {
        rawUrl.startsWith("https://", ignoreCase = true) ||
            rawUrl.startsWith("http://", ignoreCase = true) ||
            rawUrl.startsWith("mailto:", ignoreCase = true) ||
            rawUrl.startsWith("tel:", ignoreCase = true) -> rawUrl

        schemelessMessageUrl.containsMatchIn(rawUrl) -> "https://$rawUrl"

        else -> null
    }

@Composable
internal fun SelectableMessageText(
    text: String,
    onOpenUrl: (String) -> Unit,
    style: TextStyle,
    color: Color = Color.Unspecified,
    searchQuery: String = "",
    activeSearchMatch: Boolean = false,
) {
    val parsed = parseInlineMessageText(text)
    val linkColor = MaterialTheme.colorScheme.primary
    val annotated =
        buildAnnotatedString {
            append(parsed.text)
            parsed.links.forEach { link ->
                addLink(
                    LinkAnnotation.Url(
                        url = link.url,
                        styles =
                            TextLinkStyles(
                                style =
                                    SpanStyle(
                                        color = linkColor,
                                        textDecoration = TextDecoration.Underline,
                                    ),
                            ),
                        linkInteractionListener = { onOpenUrl(link.url) },
                    ),
                    start = link.start,
                    end = link.end,
                )
            }
            addSearchHighlights(parsed.text, searchQuery, activeSearchMatch)
        }
    SelectionContainer {
        Text(text = annotated, style = style, color = color)
    }
}

private fun AnnotatedString.Builder.addSearchHighlights(
    text: String,
    query: String,
    active: Boolean,
) {
    if (query.isBlank()) return
    val lower = text.lowercase()
    val needle = query.lowercase()
    var start = 0
    while (start < text.length) {
        val index = lower.indexOf(needle, start)
        if (index < 0) return
        addStyle(
            SpanStyle(
                background = if (active) Color(0xFFFFD54F) else Color(0xFFFFECB3),
                color = Color.Black,
                fontWeight = FontWeight.SemiBold,
            ),
            start = index,
            end = index + needle.length,
        )
        start = index + needle.length
    }
}
