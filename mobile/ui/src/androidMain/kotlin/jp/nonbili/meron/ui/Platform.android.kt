package jp.nonbili.meron.ui

import android.app.Activity
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.PlatformTextStyle
import androidx.core.view.WindowCompat

actual fun currentTimeMillis(): Long = System.currentTimeMillis()

actual val maskPasswordsByDefault: Boolean = true

actual val nativeTextKeyboardOptions: KeyboardOptions = KeyboardOptions.Default

actual val avatarPlatformTextStyle: PlatformTextStyle? =
    PlatformTextStyle(includeFontPadding = false)

@Composable
actual fun SyncSystemBarAppearance(dark: Boolean) {
    val view = LocalView.current
    if (view.isInEditMode) return
    SideEffect {
        val window = (view.context as? Activity)?.window ?: return@SideEffect
        WindowCompat.getInsetsController(window, view).apply {
            isAppearanceLightStatusBars = !dark
            isAppearanceLightNavigationBars = !dark
        }
    }
}
