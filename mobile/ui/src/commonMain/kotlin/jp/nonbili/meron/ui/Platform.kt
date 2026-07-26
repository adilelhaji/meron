package jp.nonbili.meron.ui

import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.PlatformTextStyle

/** Wall-clock time in milliseconds since the Unix epoch. */
expect fun currentTimeMillis(): Long

/**
 * Whether password fields should start masked. False on iOS, where masking
 * (PasswordVisualTransformation) suppresses the long-press paste menu in
 * Compose Multiplatform.
 */
expect val maskPasswordsByDefault: Boolean

/**
 * Platform-specific [KeyboardOptions] that opts into native text input on iOS
 * (enabling the system long-press context menu for paste, autofill, etc.).
 * Returns [KeyboardOptions.Default] on Android.
 */
expect val nativeTextKeyboardOptions: KeyboardOptions

/**
 * Android's default font padding shifts small avatar initials off centre.
 * Other platforms use their native text metrics unchanged.
 */
expect val avatarPlatformTextStyle: PlatformTextStyle?
