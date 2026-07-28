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
 *  at the finger. iOS ignores it: WKWebView shows the native link menu itself.
 *
 *  [fitWideContent] lets a page that is still wider than the view after the CSS
 *  reflow overrides shrink to fit instead of being clipped, the way Gmail renders
 *  fixed-width mail: the page lays out at its natural width and is scaled down,
 *  with WebView's text autosizing inflating fonts so the smaller scale stays
 *  readable. The height bridge reports pre-scale CSS pixels, so the script
 *  multiplies by the fit scale to keep reported height in dp. Only the
 *  full-screen reader enables it — in a chat bubble a 640px mail would scale to
 *  a thumbnail, so bubbles reflow only. iOS ignores it (see MailWebView.ios.kt). */
@Composable
expect fun MailWebView(
    html: String,
    modifier: Modifier,
    onContentHeight: (Dp) -> Unit,
    onOpenUrl: (String) -> Unit,
    onOpenImage: (String) -> Unit = {},
    onLinkLongPress: (String, DpOffset) -> Unit = { _, _ -> },
    fitWideContent: Boolean = false,
)
