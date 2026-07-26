package jp.nonbili.meron.ui

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.view.View
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.PopupMenu
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner

@SuppressLint("SetJavaScriptEnabled")
@Composable
actual fun MailWebView(
    html: String,
    modifier: Modifier,
    onContentHeight: (Dp) -> Unit,
    onOpenUrl: (String) -> Unit,
    onOpenImage: (String) -> Unit,
    openLinkLabel: String,
    copyLinkAddressLabel: String,
) {
    val latestOnHeight = rememberUpdatedState(onContentHeight)
    val latestOnOpenUrl = rememberUpdatedState(onOpenUrl)
    val latestOnOpenImage = rememberUpdatedState(onOpenImage)
    val latestOpenLinkLabel = rememberUpdatedState(openLinkLabel)
    val latestCopyLinkAddressLabel = rememberUpdatedState(copyLinkAddressLabel)
    // Inside NavHost this is the back-stack entry's lifecycle: RESUMED only
    // once the navigation transition has settled.
    val lifecycleState by LocalLifecycleOwner.current.lifecycle.currentStateFlow
        .collectAsState()
    AndroidView(
        modifier = modifier,
        factory = { context ->
            WebView(context).apply {
                webViewClient =
                    object : WebViewClient() {
                        override fun shouldOverrideUrlLoading(
                            view: WebView?,
                            request: WebResourceRequest?,
                        ): Boolean {
                            val url = request?.url?.toString().orEmpty()
                            if (url.isBlank()) return false
                            latestOnOpenUrl.value(url)
                            return true
                        }

                        @Deprecated("Deprecated in Android SDK")
                        override fun shouldOverrideUrlLoading(
                            view: WebView?,
                            url: String?,
                        ): Boolean {
                            if (url.isNullOrBlank()) return false
                            latestOnOpenUrl.value(url)
                            return true
                        }
                    }
                // JS is enabled to run the height-reporting script; matches the
                // desktop reader, whose iframe also runs email scripts.
                settings.javaScriptEnabled = true
                settings.domStorageEnabled = false
                settings.loadWithOverviewMode = false
                settings.useWideViewPort = false
                settings.defaultFontSize = 16
                // The view sizes to content, so it never scrolls internally.
                isVerticalScrollBarEnabled = false
                isHorizontalScrollBarEnabled = false
                setOnLongClickListener {
                    val url = webViewLinkUrl(hitTestResult.type, hitTestResult.extra) ?: return@setOnLongClickListener false
                    PopupMenu(context, this).apply {
                        menu.add(latestOpenLinkLabel.value).setOnMenuItemClickListener {
                            post { latestOnOpenUrl.value(url) }
                            true
                        }
                        menu.add(latestCopyLinkAddressLabel.value).setOnMenuItemClickListener {
                            val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                            clipboard.setPrimaryClip(ClipData.newPlainText(latestCopyLinkAddressLabel.value, url))
                            true
                        }
                        show()
                    }
                    true
                }
                addJavascriptInterface(
                    object {
                        @JavascriptInterface
                        fun report(cssPx: Int) {
                            // contentHeight comes off the JS thread; bounce to the
                            // view (main) thread before touching Compose state.
                            post { latestOnHeight.value(cssPx.dp) }
                        }
                    },
                    "MeronHeight",
                )
                addJavascriptInterface(
                    object {
                        @JavascriptInterface
                        fun open(url: String) {
                            if (url.isNotBlank()) post { latestOnOpenUrl.value(url) }
                        }
                    },
                    "MeronLink",
                )
                addJavascriptInterface(
                    object {
                        @JavascriptInterface
                        fun open(src: String) {
                            if (src.isNotBlank()) post { latestOnOpenImage.value(src) }
                        }
                    },
                    "MeronImage",
                )
            }
        },
        update = { webView ->
            // WebView draws through a GL functor that hwui cannot render into
            // the offscreen layers used by navigation transitions — doing so
            // crashes natively (SkSurface::getCanvas in GLFunctorDrawable) on
            // GL-pipeline devices. Skip drawing until the transition settles;
            // the page keeps loading and measuring while INVISIBLE.
            webView.visibility =
                if (lifecycleState.isAtLeast(Lifecycle.State.RESUMED)) View.VISIBLE else View.INVISIBLE
            if (webView.tag != html) {
                webView.tag = html
                webView.loadDataWithBaseURL(null, html, "text/html", "UTF-8", null)
            }
        },
    )
}

@Suppress("DEPRECATION")
internal fun webViewLinkUrl(
    hitType: Int,
    extra: String?,
): String? =
    extra
        ?.trim()
        ?.takeIf {
            it.isNotEmpty() &&
                (
                    hitType == WebView.HitTestResult.ANCHOR_TYPE ||
                        hitType == WebView.HitTestResult.SRC_ANCHOR_TYPE
                )
        }
