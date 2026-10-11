package top.akadesk.app.controller

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

class InputProfileStore(context: Context) {
    private val preferences = context.getSharedPreferences("akadesk-input", Context.MODE_PRIVATE)
    fun load(): List<InputShortcut> = runCatching {
        val raw = preferences.getString("shortcuts", null) ?: return InputKeys.defaults()
        val array = JSONArray(raw)
        val values = (0 until array.length()).map { index ->
            val value = array.getJSONObject(index)
            InputShortcut(value.getString("id"), value.getString("label"), value.getInt("key"),
                value.optBoolean("ctrl"),value.optBoolean("alt"),value.optBoolean("shift"),value.optBoolean("meta"))
        }
        require(InputKeys.validProfile(values))
        values
    }.getOrElse { InputKeys.defaults() }
    fun save(values: List<InputShortcut>): Boolean {
        if (!InputKeys.validProfile(values)) return false
        val array = JSONArray()
        values.forEach { array.put(JSONObject().put("id",it.id).put("label",it.label).put("key",it.key)
            .put("ctrl",it.ctrl).put("alt",it.alt).put("shift",it.shift).put("meta",it.meta)) }
        return preferences.edit().putString("shortcuts",array.toString()).commit()
    }

    fun profileNames(): List<String> = runCatching {
        val array = JSONArray(preferences.getString("profiles", "[\"默认方案\"]"))
        (0 until array.length()).map { array.getString(it) }.filter { it.isNotBlank() }.distinct().take(20)
    }.getOrDefault(listOf("默认方案"))
    fun selectedProfile(): String = preferences.getString("selectedProfile", "默认方案") ?: "默认方案"
    fun selectProfile(name: String): Boolean {
        if (name !in profileNames()) return false
        if (name == selectedProfile()) return true
        val current = preferences.getString("shortcuts", null)
        val editor = preferences.edit().putString("profile:${selectedProfile()}", current)
            .putString("selectedProfile", name).putString("shortcuts", preferences.getString("profile:$name", null))
        return editor.commit()
    }
    fun createProfile(name: String): Boolean {
        val clean = name.trim()
        val names = profileNames()
        if (clean.isEmpty() || clean.length > 32 || clean in names || names.size >= 20) return false
        return preferences.edit().putString("profiles", JSONArray(names + clean).toString()).commit()
    }
    fun position(id: String): Pair<Float, Float> =
        preferences.getFloat("x:${selectedProfile()}:$id", -1f) to preferences.getFloat("y:${selectedProfile()}:$id", -1f)
    fun savePosition(id: String, x: Float, y: Float) {
        preferences.edit().putFloat("x:${selectedProfile()}:$id", x.coerceIn(0f, 1f))
            .putFloat("y:${selectedProfile()}:$id", y.coerceIn(0f, 1f)).apply()
    }
}
