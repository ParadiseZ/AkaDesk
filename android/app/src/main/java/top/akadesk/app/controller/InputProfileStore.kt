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
            decodeBinding(value)
        }
        require(InputKeys.validProfile(values))
        values
    }.getOrElse { InputKeys.defaults() }
    fun save(values: List<InputShortcut>): Boolean {
        if (!InputKeys.validProfile(values)) return false
        val array = JSONArray()
        values.forEach { array.put(encodeBinding(it)) }
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
        if (!validName(clean)) return false
        return preferences.edit().putString("profiles", JSONArray(names + clean).toString()).putString("profile:$clean","[]").commit()
    }
    fun position(id: String): Pair<Float, Float> =
        preferences.getFloat("x:${selectedProfile()}:$id", -1f) to preferences.getFloat("y:${selectedProfile()}:$id", -1f)
    fun savePosition(id: String, x: Float, y: Float) {
        if(!x.isFinite() || !y.isFinite()) return
        preferences.edit().putFloat("x:${selectedProfile()}:$id", x.coerceIn(0f, 1f))
            .putFloat("y:${selectedProfile()}:$id", y.coerceIn(0f, 1f)).apply()
    }

    fun draft(name:String=selectedProfile()):MappingDraft {
        require(name in profileNames())
        val bindings=if(name==selectedProfile()) load() else decodeBindings(JSONArray(preferences.getString("profile:$name",null) ?: "[]"))
        val positions=bindings.mapNotNull {binding->
            val x=preferences.getFloat("x:$name:${binding.id}",-1f);val y=preferences.getFloat("y:$name:${binding.id}",-1f)
            if(x in 0f..1f && y in 0f..1f) binding.id to (x to y) else null
        }.toMap()
        return MappingDraft(name,name,bindings,positions,decodeView(JSONObject(preferences.getString("view:$name",null) ?: "{}")))
    }
    fun saveDraft(draft:MappingDraft):Boolean {
        val names=profileNames()
        if(draft.originalName!=selectedProfile() || draft.originalName !in names || !validDraft(draft) || draft.name!=draft.originalName && draft.name in names) return false
        val editor=preferences.edit()
        preferences.all.keys.filter {it.startsWith("x:${draft.originalName}:") || it.startsWith("y:${draft.originalName}:")}.forEach(editor::remove)
        editor.remove("profile:${draft.originalName}").remove("view:${draft.originalName}")
        val encoded=encodeBindings(draft.bindings).toString()
        editor.putString("profiles",JSONArray(names.map {if(it==draft.originalName) draft.name else it}).toString())
            .putString("selectedProfile",draft.name).putString("shortcuts",encoded).putString("profile:${draft.name}",encoded)
            .putString("view:${draft.name}",encodeView(draft.view).toString())
        draft.positions.forEach {(id,point)->editor.putFloat("x:${draft.name}:$id",point.first).putFloat("y:${draft.name}:$id",point.second)}
        return editor.commit()
    }
    fun deleteProfile(name:String):Boolean {
        val names=profileNames();if(name !in names || names.size<=1) return false
        val remaining=names-name;val editor=preferences.edit().putString("profiles",JSONArray(remaining).toString()).remove("profile:$name").remove("view:$name")
        preferences.all.keys.filter {it.startsWith("x:$name:") || it.startsWith("y:$name:")}.forEach(editor::remove)
        if(name==selectedProfile()) editor.putString("selectedProfile",remaining.first()).putString("shortcuts",preferences.getString("profile:${remaining.first()}",null))
        return editor.commit()
    }
    fun exportProfile(name:String=selectedProfile()):String {
        val value=draft(name);val positions=JSONObject()
        value.positions.forEach {(id,point)->positions.put(id,JSONObject().put("x",point.first).put("y",point.second))}
        return JSONObject().put("version",1).put("name",name).put("bindings",encodeBindings(value.bindings)).put("positions",positions).put("view",encodeView(value.view)).toString(2)
    }
    fun importProfile(raw:String):String {
        require(raw.toByteArray().size<=65536) {"方案文件过大"}
        val value=JSONObject(raw);require(value.getInt("version")==1) {"不支持的方案版本"}
        val base=value.getString("name").trim();require(validName(base)) {"方案名称无效"}
        val names=profileNames();require(names.size<20) {"最多保存 20 个方案"}
        var name=base;var suffix=2
        while(name in names) {name=base.take(25)+" (${suffix++})"}
        val bindings=decodeBindings(value.getJSONArray("bindings"));val positions=mutableMapOf<String,Pair<Float,Float>>()
        val points=value.optJSONObject("positions") ?: JSONObject()
        for(binding in bindings) points.optJSONObject(binding.id)?.let {positions[binding.id]=it.getDouble("x").toFloat() to it.getDouble("y").toFloat()}
        val draft=MappingDraft(name,name,bindings,positions,decodeView(value.optJSONObject("view") ?: JSONObject()))
        require(validDraft(draft)) {"方案内容无效"}
        val editor=preferences.edit().putString("profiles",JSONArray(names+name).toString()).putString("profile:$name",encodeBindings(bindings).toString()).putString("view:$name",encodeView(draft.view).toString())
        positions.forEach {(id,p)->editor.putFloat("x:$name:$id",p.first).putFloat("y:$name:$id",p.second)}
        check(editor.commit()) {"保存失败"};return name
    }
    private fun validName(name:String)=name.isNotBlank() && name.length<=32 && name.none {it==':' || it.isISOControl()}
    private fun validDraft(value:MappingDraft)=validName(value.name) && InputKeys.validProfile(value.bindings) && value.view.valid() && value.positions.all {(id,p)->
        value.bindings.any {it.id==id} && p.first.isFinite() && p.second.isFinite() && p.first in 0f..1f && p.second in 0f..1f
    }
    private fun encodeBindings(values:List<InputShortcut>)=JSONArray().apply {values.forEach {put(encodeBinding(it))}}
    private fun decodeBindings(array:JSONArray):List<InputShortcut> {
        require(array.length()<=30)
        return (0 until array.length()).map {decodeBinding(array.getJSONObject(it))}.also {require(InputKeys.validProfile(it))}
    }
    private fun encodeBinding(value:InputShortcut)=JSONObject().put("id",value.id).put("label",value.label).put("key",value.key)
        .put("ctrl",value.ctrl).put("alt",value.alt).put("shift",value.shift).put("meta",value.meta)
        .put("actions",JSONArray().apply {value.actions.forEach {put(JSONObject().put("kind",it.kind.name).put("code",it.code))}})
        .put("behavior",value.behavior.name).put("size",value.size).put("opacity",value.opacity).put("movable",value.movable)
    private fun decodeBinding(value:JSONObject):InputShortcut {
        val actions=value.optJSONArray("actions") ?: JSONArray();require(actions.length()<=4)
        return InputShortcut(value.getString("id"),value.getString("label"),value.getInt("key"),value.optBoolean("ctrl"),value.optBoolean("alt"),value.optBoolean("shift"),value.optBoolean("meta"),
            (0 until actions.length()).map {val item=actions.getJSONObject(it);MappingAction(MappingActionKind.valueOf(item.getString("kind")),item.getInt("code"))},
            MappingBehavior.valueOf(value.optString("behavior",MappingBehavior.Hold.name)),
            if(value.isNull("size")) null else value.getInt("size"),if(value.isNull("opacity")) null else value.getDouble("opacity").toFloat(),value.optBoolean("movable"))
    }
    private fun encodeView(value:MappingViewControl)=JSONObject().put("enabled",value.enabled).put("trigger",value.trigger.name).put("rightHalf",value.rightHalf)
        .put("x",value.sensitivityX).put("y",value.sensitivityY).put("hideButtons",value.hideButtons).put("disableDoubleTap",value.disableDoubleTap)
    private fun decodeView(value:JSONObject)=MappingViewControl(value.optBoolean("enabled"),ViewTrigger.valueOf(value.optString("trigger",ViewTrigger.Move.name)),value.optBoolean("rightHalf"),
        value.optDouble("x",1.0).toFloat(),value.optDouble("y",1.0).toFloat(),value.optBoolean("hideButtons"),value.optBoolean("disableDoubleTap")).also {require(it.valid())}
}
