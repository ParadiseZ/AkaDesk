package top.akadesk.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import top.akadesk.app.controller.ControllerCoordinator
import top.akadesk.app.ui.home.ControllerHomeApp
import top.akadesk.app.ui.theme.AkaDeskTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            AkaDeskTheme {
                val coordinator = remember { ControllerCoordinator(this) }
                DisposableEffect(Unit) {
                    onDispose { coordinator.stopDiscovery() }
                }
                ControllerHomeApp(coordinator = coordinator)
            }
        }
    }
}
