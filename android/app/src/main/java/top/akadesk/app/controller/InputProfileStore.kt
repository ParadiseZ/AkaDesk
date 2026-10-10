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
}
