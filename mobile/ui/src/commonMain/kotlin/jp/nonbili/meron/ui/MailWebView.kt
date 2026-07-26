package jp.nonbili.meron.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpOffset

/** Renders a complete HTML document (mail body). WKWebView on iOS, WebView on
 *  Android. JavaScript is enabled so the embedded measurement script can report
 *  content height via [onContentHeight] (in dp), letting the caller size the
 *  view to fit the email.
 *
 *  [onLinkLongPress] reports a long press on a link, with the press position
 *  relative to the view's top-left corner, so the caller can show its own menu
 *  at the finger. iOS ignores it: WKWebView shows the native link menu itself. */
@Composable
expect fun MailWebView(
    html: String,
    modifier: Modifier,
    onContentHeight: (Dp) -> Unit,
    onOpenUrl: (String) -> Unit,
    onOpenImage: (String) -> Unit = {},
    onLinkLongPress: (String, DpOffset) -> Unit = { _, _ -> },
)
