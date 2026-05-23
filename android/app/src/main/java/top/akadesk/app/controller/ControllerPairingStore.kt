package top.akadesk.app.controller

import android.content.Context
import java.util.UUID

class ControllerPairingStore(context: Context) {
    private val prefs = context.getSharedPreferences("akadesk-controller", Context.MODE_PRIVATE)

    fun controllerDeviceId(): String {
        val existing = prefs.getString(KEY_CONTROLLER_DEVICE_ID, null)
        if (!existing.isNullOrBlank()) {
            return existing
        }
        val created = "android-${UUID.randomUUID()}"
        prefs.edit().putString(KEY_CONTROLLER_DEVICE_ID, created).apply()
        return created
    }

    fun pairToken(deviceId: String): String? = prefs.getString("pair_token_$deviceId", null)

    fun savePairToken(deviceId: String, pairToken: String) {
        prefs.edit().putString("pair_token_$deviceId", pairToken).apply()
    }

    companion object {
        private const val KEY_CONTROLLER_DEVICE_ID = "controller_device_id"
    }
}
