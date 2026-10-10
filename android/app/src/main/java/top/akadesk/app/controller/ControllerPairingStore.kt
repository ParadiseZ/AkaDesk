package top.akadesk.app.controller

import android.content.Context

class ControllerPairingStore(context: Context) {
    private val prefs = context.getSharedPreferences("akadesk-controller", Context.MODE_PRIVATE)

    init {
        migrateDeviceIds()
    }

    private fun migrateDeviceIds() {
        val stored = prefs.all
        val editor = prefs.edit()
        var changed = false
        for (listKey in listOf(KEY_PAIRED_DEVICE_IDS, KEY_FAVORITE_DEVICE_IDS, KEY_HISTORY_DEVICE_IDS)) {
            val ids = prefs.getStringSet(listKey, emptySet()).orEmpty()
            val migrated = ids.map(NumericDeviceId::migrate).toSet()
            check(migrated.size == ids.size) { "Device ID migration collision" }
            for (oldId in ids) {
                val numericId = NumericDeviceId.migrate(oldId)
                if (numericId == oldId) continue
                changed = true
                val prefix = when (listKey) {
                    KEY_PAIRED_DEVICE_IDS -> "pair_"
                    KEY_FAVORITE_DEVICE_IDS -> "favorite_"
                    else -> "history_"
                }
                for ((key, value) in stored) {
                    if (!key.startsWith(prefix) || !key.endsWith("_$oldId")) continue
                    val migratedKey = key.removeSuffix(oldId) + numericId
                    when (value) {
                        is String -> editor.putString(migratedKey, value)
                        is Long -> editor.putLong(migratedKey, value)
                    }
                    editor.remove(key)
                }
            }
            if (migrated != ids) editor.putStringSet(listKey, migrated)
        }
        if (changed) check(editor.commit()) { "Unable to persist device ID migration" }
    }

    fun controllerDeviceId(): String {
        val existing = prefs.getString(KEY_CONTROLLER_DEVICE_ID, null)
        if (!existing.isNullOrBlank()) {
            val numericId = NumericDeviceId.migrate(existing)
            if (numericId != existing) check(prefs.edit().putString(KEY_CONTROLLER_DEVICE_ID, numericId).commit())
            return numericId
        }
        val created = NumericDeviceId.create()
        prefs.edit().putString(KEY_CONTROLLER_DEVICE_ID, created).apply()
        return created
    }

    fun pairToken(deviceId: String): String? = prefs.getString("pair_token_$deviceId", null)

    fun hasPairToken(deviceId: String): Boolean = !pairToken(deviceId).isNullOrBlank()

    fun savePairToken(
        deviceId: String,
        pairToken: String,
        displayName: String,
        hostAddress: String,
        platform: String,
    ) {
        val ids = pairedDeviceIds().toMutableSet()
        ids += deviceId
        prefs.edit()
            .putString("pair_token_$deviceId", pairToken)
            .putString("pair_display_$deviceId", displayName)
            .putString("pair_host_$deviceId", hostAddress)
            .putString("pair_platform_$deviceId", platform)
            .putLong("pair_saved_$deviceId", System.currentTimeMillis())
            .putStringSet(KEY_PAIRED_DEVICE_IDS, ids)
            .apply()
    }

    fun refreshPairRecord(deviceId: String, displayName: String, hostAddress: String, platform: String) {
        if (!hasPairToken(deviceId)) {
            return
        }
        val ids = pairedDeviceIds().toMutableSet()
        ids += deviceId
        prefs.edit()
            .putString("pair_display_$deviceId", displayName)
            .putString("pair_host_$deviceId", hostAddress)
            .putString("pair_platform_$deviceId", platform)
            .putStringSet(KEY_PAIRED_DEVICE_IDS, ids)
            .apply()
    }

    fun removePairing(deviceId: String) {
        val ids = pairedDeviceIds().toMutableSet()
        ids.remove(deviceId)
        prefs.edit()
            .remove("pair_token_$deviceId")
            .remove("pair_display_$deviceId")
            .remove("pair_host_$deviceId")
            .remove("pair_platform_$deviceId")
            .remove("pair_saved_$deviceId")
            .putStringSet(KEY_PAIRED_DEVICE_IDS, ids)
            .apply()
    }

    fun pairedDevices(): List<ControllerPairRecord> {
        return pairedDeviceIds()
            .map { deviceId ->
                ControllerPairRecord(
                    deviceId = deviceId,
                    displayName = prefs.getString("pair_display_$deviceId", null).orEmpty().ifBlank { deviceId },
                    hostAddress = prefs.getString("pair_host_$deviceId", null).orEmpty(),
                    platform = prefs.getString("pair_platform_$deviceId", null).orEmpty(),
                    savedAtMs = prefs.getLong("pair_saved_$deviceId", 0L),
                )
            }
            .sortedByDescending { it.savedAtMs }
    }

    fun preferredVideoCodec(): ControllerVideoCodecPreset {
        return ControllerVideoCodecPreset.fromStorageKey(prefs.getString(KEY_VIDEO_CODEC_PRESET, null))
    }

    fun savePreferredVideoCodec(preset: ControllerVideoCodecPreset) {
        prefs.edit().putString(KEY_VIDEO_CODEC_PRESET, preset.storageKey).apply()
    }

    fun addFavorite(host: ControlledHostSummary) {
        val ids = favoriteDeviceIds().toMutableSet()
        ids += host.deviceId
        prefs.edit()
            .putString("favorite_display_${host.deviceId}", host.displayName)
            .putString("favorite_host_${host.deviceId}", host.hostAddress)
            .putString("favorite_platform_${host.deviceId}", host.platform)
            .putLong("favorite_saved_${host.deviceId}", System.currentTimeMillis())
            .putStringSet(KEY_FAVORITE_DEVICE_IDS, ids)
            .apply()
    }

    fun removeFavorite(deviceId: String) {
        val ids = favoriteDeviceIds().toMutableSet()
        ids.remove(deviceId)
        prefs.edit()
            .remove("favorite_display_$deviceId")
            .remove("favorite_host_$deviceId")
            .remove("favorite_platform_$deviceId")
            .remove("favorite_saved_$deviceId")
            .putStringSet(KEY_FAVORITE_DEVICE_IDS, ids)
            .apply()
    }

    fun isFavorite(deviceId: String): Boolean = favoriteDeviceIds().contains(deviceId)

    fun favoriteDevices(): List<ControllerPairRecord> {
        return favoriteDeviceIds()
            .map { deviceId ->
                ControllerPairRecord(
                    deviceId = deviceId,
                    displayName = prefs.getString("favorite_display_$deviceId", null).orEmpty().ifBlank { deviceId },
                    hostAddress = prefs.getString("favorite_host_$deviceId", null).orEmpty(),
                    platform = prefs.getString("favorite_platform_$deviceId", null).orEmpty(),
                    savedAtMs = prefs.getLong("favorite_saved_$deviceId", 0L),
                )
            }
            .sortedByDescending { it.savedAtMs }
    }

    fun addHistory(host: ControlledHostSummary) {
        val ids = historyDeviceIds().toMutableSet()
        ids.remove(host.deviceId)
        ids += host.deviceId
        prefs.edit()
            .putString("history_display_${host.deviceId}", host.displayName)
            .putString("history_host_${host.deviceId}", host.hostAddress)
            .putString("history_platform_${host.deviceId}", host.platform)
            .putLong("history_saved_${host.deviceId}", System.currentTimeMillis())
            .putStringSet(KEY_HISTORY_DEVICE_IDS, ids)
            .apply()
    }

    fun removeHistory(deviceId: String) {
        val ids = historyDeviceIds().toMutableSet()
        ids.remove(deviceId)
        prefs.edit()
            .remove("history_display_$deviceId")
            .remove("history_host_$deviceId")
            .remove("history_platform_$deviceId")
            .remove("history_saved_$deviceId")
            .putStringSet(KEY_HISTORY_DEVICE_IDS, ids)
            .apply()
    }

    fun historyDevices(): List<ControllerPairRecord> {
        return historyDeviceIds()
            .map { deviceId ->
                ControllerPairRecord(
                    deviceId = deviceId,
                    displayName = prefs.getString("history_display_$deviceId", null).orEmpty().ifBlank { deviceId },
                    hostAddress = prefs.getString("history_host_$deviceId", null).orEmpty(),
                    platform = prefs.getString("history_platform_$deviceId", null).orEmpty(),
                    savedAtMs = prefs.getLong("history_saved_$deviceId", 0L),
                )
            }
            .sortedByDescending { it.savedAtMs }
    }

    private fun pairedDeviceIds(): Set<String> {
        return prefs.getStringSet(KEY_PAIRED_DEVICE_IDS, emptySet()).orEmpty()
    }

    private fun favoriteDeviceIds(): Set<String> {
        return prefs.getStringSet(KEY_FAVORITE_DEVICE_IDS, emptySet()).orEmpty()
    }

    private fun historyDeviceIds(): Set<String> {
        return prefs.getStringSet(KEY_HISTORY_DEVICE_IDS, emptySet()).orEmpty()
    }

    companion object {
        private const val KEY_CONTROLLER_DEVICE_ID = "controller_device_id"
        private const val KEY_PAIRED_DEVICE_IDS = "paired_device_ids"
        private const val KEY_FAVORITE_DEVICE_IDS = "favorite_device_ids"
        private const val KEY_HISTORY_DEVICE_IDS = "history_device_ids"
        private const val KEY_VIDEO_CODEC_PRESET = "video_codec_preset"
    }
}
