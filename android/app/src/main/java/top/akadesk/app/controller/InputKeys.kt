package top.akadesk.app.controller

data class InputShortcut(
    val id: String,
    val label: String,
    val key: Int,
    val ctrl: Boolean = false,
    val alt: Boolean = false,
    val shift: Boolean = false,
    val meta: Boolean = false,
) {
    fun chordLabel(): String = listOfNotNull(if(ctrl) "Ctrl" else null, if(alt) "Alt" else null,
        if(shift) "Shift" else null, if(meta) "Win" else null, InputKeys.catalog[key]).joinToString(" + ")
}

object InputKeys {
    val catalog: Map<Int, String> = linkedMapOf(
        0x0d to "Enter", 0x1b to "Esc", 0x09 to "Tab", 0x08 to "Backspace", 0x20 to "Space",
        0x2e to "Delete", 0x2d to "Insert", 0x24 to "Home", 0x23 to "End", 0x21 to "Page Up", 0x22 to "Page Down",
        0x25 to "←", 0x26 to "↑", 0x27 to "→", 0x28 to "↓",
    ) + (0x41..0x5a).associateWith { it.toChar().toString() } + (0x30..0x39).associateWith { it.toChar().toString() } +
        (0x70..0x87).associateWith { "F${it-0x6f}" } + mapOf(0x10 to "Shift",0x11 to "Ctrl",0x12 to "Alt",0x5b to "Win")
    fun valid(shortcut: InputShortcut): Boolean = shortcut.id.isNotBlank() && shortcut.id.length<=64 &&
        shortcut.label.isNotBlank() && shortcut.label.length<=32 && shortcut.key in catalog
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
