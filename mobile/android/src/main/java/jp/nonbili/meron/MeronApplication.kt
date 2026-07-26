package jp.nonbili.meron

import android.app.Application

/**
 * Process-wide setup that must be in place before any component runs — the
 * activity, the push service, and the background sync worker all start from
 * here, and a crash in any of them should reach the diagnostic log.
 */
class MeronApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        // Native DNS discovery needs the application Context before any core
        // command can reach Hickory's Android system resolver.
        MeronCoreNative.initializeAndroidContext(this)
        AndroidCrashLog.install(this)
        AndroidSyncDiagnosticLog.installUiLogSink(this)
    }
}
