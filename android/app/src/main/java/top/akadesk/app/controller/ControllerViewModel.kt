package top.akadesk.app.controller

import android.app.Application
import androidx.lifecycle.AndroidViewModel

/** Keeps the connection alive while Android recreates the screen on rotation. */
class ControllerViewModel(application: Application) : AndroidViewModel(application) {
    val coordinator = ControllerCoordinator(application)

    override fun onCleared() {
        coordinator.stopDiscovery()
        super.onCleared()
    }
}
