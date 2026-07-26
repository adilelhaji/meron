package jp.nonbili.meron.ui

import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.PlatformTextStyle

actual fun currentTimeMillis(): Long = System.currentTimeMillis()

actual val maskPasswordsByDefault: Boolean = true

actual val nativeTextKeyboardOptions: KeyboardOptions = KeyboardOptions.Default

actual val avatarPlatformTextStyle: PlatformTextStyle? =
    PlatformTextStyle(includeFontPadding = false)
