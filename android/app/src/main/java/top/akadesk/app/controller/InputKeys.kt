package top.akadesk.app.controller

enum class MappingActionKind { Key, Mouse, Wheel }
enum class MappingBehavior(val title:String) { Hold("按住"), Tap("单击"), Toggle("锁定"), Repeat("连发") }
data class MappingAction(val kind:MappingActionKind,val code:Int) {
    fun label():String=when(kind) {
        MappingActionKind.Key->InputKeys.catalog[code] ?: ""
        MappingActionKind.Mouse->mapOf(1 to "左键",2 to "右键",3 to "中键")[code] ?: ""
        MappingActionKind.Wheel->if(code>0) "滚轮↑" else "滚轮↓"
    }
    fun valid():Boolean=when(kind) {
        MappingActionKind.Key->code in InputKeys.catalog
        MappingActionKind.Mouse->code in 1..3
        MappingActionKind.Wheel->code in listOf(-1,1)
    }
}
enum class ViewTrigger(val title:String) { Move("跟随鼠标移动"), Left("左键按住拖拽"), Right("右键按住拖拽"), Middle("中键按住拖拽") }
data class MappingViewControl(val enabled:Boolean=false,val trigger:ViewTrigger=ViewTrigger.Move,
    val rightHalf:Boolean=false,val sensitivityX:Float=1f,val sensitivityY:Float=1f,
    val hideButtons:Boolean=false,val disableDoubleTap:Boolean=false) {
    fun valid()=sensitivityX.isFinite() && sensitivityY.isFinite() && sensitivityX in .1f..3f && sensitivityY in .1f..3f
}
data class MappingDraft(val originalName:String,val name:String,val bindings:List<InputShortcut>,
    val positions:Map<String,Pair<Float,Float>>,val view:MappingViewControl)

data class InputShortcut(
    val id: String,
    val label: String,
    val key: Int,
    val ctrl: Boolean = false,
    val alt: Boolean = false,
    val shift: Boolean = false,
    val meta: Boolean = false,
    val actions:List<MappingAction> = emptyList(),
    val behavior:MappingBehavior = MappingBehavior.Hold,
    val size:Int? = null,
    val opacity:Float? = null,
    val movable:Boolean = false,
) {
    fun effectiveActions():List<MappingAction> = actions.ifEmpty {
        (listOfNotNull(if(ctrl) 0x11 else null,if(alt) 0x12 else null,if(shift) 0x10 else null,if(meta) 0x5b else null)+key)
            .distinct().map {MappingAction(MappingActionKind.Key,it)}
    }
    fun chordLabel(): String = effectiveActions().joinToString(" + ") {it.label()}
}

object InputKeys {
    fun defaultPosition(index:Int,count:Int):Pair<Float,Float> {
        val columns=if(count>14) 4 else 2
        val rows=(count+columns-1)/columns
        val x=if(columns==2) listOf(.04f,.90f) else listOf(.04f,.90f,.12f,.82f)
        return x[index%columns] to (.10f+(index/columns)*(.80f/(rows-1).coerceAtLeast(1)))
    }
    val catalog: Map<Int, String> = linkedMapOf(
        0x0d to "Enter", 0x1b to "Esc", 0x09 to "Tab", 0x08 to "Backspace", 0x20 to "Space",
        0x2e to "Delete", 0x2d to "Insert", 0x24 to "Home", 0x23 to "End", 0x21 to "Page Up", 0x22 to "Page Down",
        0x25 to "←", 0x26 to "↑", 0x27 to "→", 0x28 to "↓",
        0x14 to "Caps Lock", 0x2c to "Print Screen", 0x91 to "Scroll Lock", 0x13 to "Pause",
        0xba to ";", 0xbb to "=", 0xbc to ",", 0xbd to "-", 0xbe to ".", 0xbf to "/",
        0xc0 to "`", 0xdb to "[", 0xdc to "\\", 0xdd to "]", 0xde to "'",
    ) + (0x41..0x5a).associateWith { it.toChar().toString() } + (0x30..0x39).associateWith { it.toChar().toString() } +
        (0x70..0x87).associateWith { "F${it-0x6f}" } + mapOf(0x10 to "Shift",0x11 to "Ctrl",0x12 to "Alt",0x5b to "Win")
    fun valid(shortcut: InputShortcut): Boolean = shortcut.id.isNotBlank() && shortcut.id.length<=64 &&
        shortcut.label.isNotBlank() && shortcut.label.length<=32 && shortcut.key in catalog &&
        shortcut.actions.size<=4 && shortcut.actions.all {it.valid()} && shortcut.actions.distinct().size==shortcut.actions.size &&
        (shortcut.size==null || shortcut.size in 16..100) &&
        (shortcut.opacity==null || shortcut.opacity.isFinite() && shortcut.opacity in .1f..1f) &&
        !(shortcut.behavior==MappingBehavior.Toggle && shortcut.effectiveActions().any {it.kind==MappingActionKind.Wheel})
    fun validProfile(values: List<InputShortcut>): Boolean = values.size<=30 && values.all(::valid) && values.map {it.id}.distinct().size==values.size
    fun defaults() = listOf(
        InputShortcut("copy","复制",0x43,ctrl=true), InputShortcut("paste","粘贴",0x56,ctrl=true),
        InputShortcut("undo","撤销",0x5a,ctrl=true), InputShortcut("select","全选",0x41,ctrl=true),
        InputShortcut("enter","Enter",0x0d), InputShortcut("esc","Esc",0x1b), InputShortcut("tab","Tab",0x09),
        InputShortcut("backspace","退格",0x08), InputShortcut("left","←",0x25), InputShortcut("up","↑",0x26),
        InputShortcut("right","→",0x27), InputShortcut("down","↓",0x28),
    )
    // Android hardware-key codes -> Windows virtual keys; Android system keys stay local.
    fun fromAndroid(code: Int): Int? = when(code) {
        in 29..54 -> 0x41+code-29
        in 7..16 -> 0x30+code-7
        in 131..142 -> 0x70+code-131
        19->0x26; 20->0x28; 21->0x25; 22->0x27
        61->0x09; 62->0x20; 66->0x0d; 67->0x08; 111->0x1b; 112->0x2e
        113,114->0x11; 59,60->0x10; 57,58->0x12; 117,118->0x5b
        92->0x21; 93->0x22; 122->0x24; 123->0x23; 124->0x2d
        55->0xbc; 56->0xbe; 68->0xc0; 69->0xbd; 70->0xbb
        71->0xdb; 72->0xdd; 73->0xdc; 74->0xba; 75->0xde; 76->0xbf
        else->null
    }
}
