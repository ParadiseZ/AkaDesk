package top.akadesk.app.controller

import android.content.Context

enum class PointerMode { Touch, Trackpad }
enum class ToolbarHide { Mapping, Always, Never }

data class RemoteControlPreferences(
    val pointerMode: PointerMode = PointerMode.Touch,
    val virtualMouse: Boolean = false,
    val tabletTouch: Boolean = false,
    val mappingEnabled: Boolean = false,
    val toolbarHide: ToolbarHide = ToolbarHide.Mapping,
    val showNetwork: Boolean = false,
    val avoidCutout: Boolean = true,
    val autoPip: Boolean = false,
    val showPipShortcut: Boolean = true,
    val pointerSpeed: Float = 1f,
    val buttonOpacity: Float = 0.75f,
    val buttonSize: Int = 48,
    val pinchZoom: Boolean = true,
    val buttonHaptics: Boolean = false,
)

class RemoteControlPreferenceStore(context: Context) {
    private val prefs = context.getSharedPreferences("akadesk-remote-controls", Context.MODE_PRIVATE)
    fun load() = RemoteControlPreferences(
        pointerMode = PointerMode.entries.firstOrNull { it.name == prefs.getString("pointerMode", null) } ?: PointerMode.Touch,
        virtualMouse = prefs.getBoolean("virtualMouse", false),
        tabletTouch = prefs.getBoolean("tabletTouch", false),
        mappingEnabled = prefs.getBoolean("mappingEnabled", false),
        toolbarHide = ToolbarHide.entries.firstOrNull { it.name == prefs.getString("toolbarHide", null) } ?: ToolbarHide.Mapping,
        showNetwork = prefs.getBoolean("showNetwork", false),
        avoidCutout = prefs.getBoolean("avoidCutout", true),
        autoPip = prefs.getBoolean("autoPip", false),
        showPipShortcut = prefs.getBoolean("showPipShortcut", true),
        pointerSpeed = prefs.getFloat("pointerSpeed", 1f).coerceIn(0.25f, 3f),
        buttonOpacity = prefs.getFloat("buttonOpacity", .75f).coerceIn(.2f, 1f),
        buttonSize = prefs.getInt("buttonSize", 48).coerceIn(32, 72),
        pinchZoom = prefs.getBoolean("pinchZoom", true),
        buttonHaptics = prefs.getBoolean("buttonHaptics", false),
    )
    fun save(value: RemoteControlPreferences) = prefs.edit()
        .putString("pointerMode", value.pointerMode.name).putBoolean("virtualMouse", value.virtualMouse)
        .putBoolean("tabletTouch", value.tabletTouch)
        .putBoolean("mappingEnabled", value.mappingEnabled).putString("toolbarHide", value.toolbarHide.name)
        .putBoolean("showNetwork", value.showNetwork).putBoolean("avoidCutout", value.avoidCutout)
        .putBoolean("autoPip", value.autoPip).putBoolean("showPipShortcut", value.showPipShortcut)
        .putFloat("pointerSpeed", value.pointerSpeed).putFloat("buttonOpacity", value.buttonOpacity)
        .putInt("buttonSize", value.buttonSize).putBoolean("pinchZoom",value.pinchZoom).putBoolean("buttonHaptics",value.buttonHaptics).commit()
}
