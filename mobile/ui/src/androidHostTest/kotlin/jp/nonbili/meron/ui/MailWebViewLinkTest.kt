package jp.nonbili.meron.ui

import android.webkit.WebView
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

@Suppress("DEPRECATION")
class MailWebViewLinkTest {
    @Test
    fun acceptsAnchorHitTestResults() {
        assertEquals(
            "https://example.com",
            webViewLinkUrl(WebView.HitTestResult.ANCHOR_TYPE, " https://example.com "),
        )
        assertEquals(
            "https://example.com/image",
            webViewLinkUrl(WebView.HitTestResult.SRC_ANCHOR_TYPE, "https://example.com/image"),
        )
    }

    @Test
    fun rejectsNonLinksAndBlankTargets() {
        assertNull(webViewLinkUrl(WebView.HitTestResult.IMAGE_TYPE, "https://example.com/image.png"))
        assertNull(webViewLinkUrl(WebView.HitTestResult.ANCHOR_TYPE, "  "))
        assertNull(webViewLinkUrl(WebView.HitTestResult.ANCHOR_TYPE, null))
    }
}
