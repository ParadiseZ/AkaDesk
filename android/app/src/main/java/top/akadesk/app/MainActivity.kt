package top.akadesk.app

import android.os.Bundle
import android.content.pm.ActivityInfo
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.ViewModelProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import top.akadesk.app.controller.ControllerViewModel
import top.akadesk.app.ui.home.ControllerHomeApp
import top.akadesk.app.ui.theme.AkaDeskTheme

class MainActivity : ComponentActivity() {
    private lateinit var coordinator: top.akadesk.app.controller.ControllerCoordinator
    private var originalOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        originalOrientation = savedInstanceState?.getInt("normal_orientation") ?: requestedOrientation
        enableEdgeToEdge()
        coordinator = ViewModelProvider(this)[ControllerViewModel::class.java].coordinator
        setContent {
            val viewing = coordinator.activeSession != null
            LaunchedEffect(viewing, coordinator.remotePortrait) {
                requestedOrientation = if (viewing) {
                    if (coordinator.remotePortrait) ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT else ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
                } else originalOrientation
                val bars = WindowCompat.getInsetsController(window, window.decorView)
                if (viewing) {
                    bars.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                    bars.hide(WindowInsetsCompat.Type.systemBars())
                    window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                } else {
                    bars.show(WindowInsetsCompat.Type.systemBars())
                    window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                }
            }
            AkaDeskTheme {
                ControllerHomeApp(coordinator = coordinator)
            }
        }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        outState.putInt("normal_orientation", originalOrientation)
        super.onSaveInstanceState(outState)
    }
    override fun onStart() {
        super.onStart()
        if (::coordinator.isInitialized) coordinator.inputForeground = true
    }
    override fun onStop() {
        if (::coordinator.isInitialized) {
            coordinator.inputForeground = false
            coordinator.releaseRemoteInput()
        }
        super.onStop()
    }
    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        if (::coordinator.isInitialized && coordinator.activeSession != null && coordinator.remotePreferences.autoPip) enterRemotePip()
    }
    fun enterRemotePip(): Boolean {
        if (coordinator.activeSession == null || !packageManager.hasSystemFeature(android.content.pm.PackageManager.FEATURE_PICTURE_IN_PICTURE)) return false
        coordinator.releaseRemoteInput()
        val ratio = coordinator.activeVideoAspectRatio.coerceIn(1f / 2.39f, 2.39f)
        return runCatching { enterPictureInPictureMode(android.app.PictureInPictureParams.Builder()
            .setAspectRatio(android.util.Rational((ratio * 1000).toInt(), 1000)).build()) }.getOrDefault(false)
    }
    override fun onPictureInPictureModeChanged(isInPictureInPictureMode: Boolean, newConfig: android.content.res.Configuration) {
        super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        if (::coordinator.isInitialized) {
            coordinator.pictureInPicture = isInPictureInPictureMode
            coordinator.inputForeground = !isInPictureInPictureMode
            coordinator.releaseRemoteInput()
        }
    }
    override fun dispatchKeyEvent(event: android.view.KeyEvent): Boolean {
        if (::coordinator.isInitialized && coordinator.hardwareKey(event)) return true
        return super.dispatchKeyEvent(event)
    }
}
