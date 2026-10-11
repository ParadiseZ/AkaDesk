package top.akadesk.app.ui.home

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import top.akadesk.app.controller.*

@Composable
fun RemoteKeyboard(coordinator: ControllerCoordinator, onClose: () -> Unit, onEdit: () -> Unit) {
    var page by rememberSaveable { mutableIntStateOf(0) }
    var text by rememberSaveable { mutableStateOf("") }
    var ctrl by rememberSaveable { mutableStateOf(false) }
    var shift by rememberSaveable { mutableStateOf(false) }
    var alt by rememberSaveable { mutableStateOf(false) }
    var meta by rememberSaveable { mutableStateOf(false) }
    val enabled = coordinator.remoteInputReady && !coordinator.viewOnly
    Column(Modifier.fillMaxWidth().heightIn(max=260.dp).verticalScroll(rememberScrollState()).padding(8.dp), verticalArrangement=Arrangement.spacedBy(4.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement=Arrangement.SpaceEvenly) {
            listOf("输入法", "快捷键", "电脑键盘").forEachIndexed { index, title ->
                TextButton(onClick={page=index; coordinator.releaseRemoteInput()}) { Text(title, color=if(page==index) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface) }
            }
            TextButton(onClick=onClose) {Text("关闭")}
        }
        when(page) {
            0 -> Row(Modifier.fillMaxWidth(), horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(value=text,onValueChange={text=it},label={Text("输入文字")},maxLines=2,modifier=Modifier.weight(1f))
                Button(enabled=enabled && text.isNotEmpty() && text.toByteArray().size<=4096,onClick={if(coordinator.sendText(text)) text=""}) {Text("发送文字")}
            }
            1 -> {
                TextButton(onClick=onEdit) {Text("编辑映射 · ${coordinator.selectedInputProfile}")}
                val shortcuts = listOf(
                    InputShortcut("switch","切换窗口",0x09,alt=true), InputShortcut("desktop","显示桌面",0x44,meta=true),
                    InputShortcut("task","所有窗口",0x09,meta=true), InputShortcut("save","保存",0x53,ctrl=true),
                    InputShortcut("cut","剪切",0x58,ctrl=true), InputShortcut("lock","锁屏",0x4c,meta=true),
                    InputShortcut("manager","任务管理器",0x1b,ctrl=true,shift=true),
                ) + coordinator.inputShortcuts
                shortcuts.chunked(6).forEach { row ->
                    Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(4.dp)) {
                        row.forEach { key -> Button(onClick={coordinator.sendShortcut(key)},enabled=enabled,modifier=Modifier.weight(1f),contentPadding=PaddingValues(4.dp)) {Text(key.label, maxLines=1, style=MaterialTheme.typography.labelSmall)} }
                    }
                }
            }
            2 -> {
                Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(4.dp)) {
                    listOf("Ctrl", "Shift", "Alt", "Win").forEachIndexed { index, label ->
                        val selected = listOf(ctrl,shift,alt,meta)[index]
                        FilterChip(selected=selected,onClick={when(index){0->ctrl=!ctrl;1->shift=!shift;2->alt=!alt;3->meta=!meta}},label={Text(label)},modifier=Modifier.weight(1f))
                    }
                }
                val rows = listOf(
                    listOf(0x1b,0x70,0x71,0x72,0x73,0x74,0x75,0x76,0x77,0x78,0x79,0x7a,0x7b),
                    listOf(0xc0,0x31,0x32,0x33,0x34,0x35,0x36,0x37,0x38,0x39,0x30,0xbd,0xbb,0x08),
                    listOf(0x09,0x51,0x57,0x45,0x52,0x54,0x59,0x55,0x49,0x4f,0x50,0xdb,0xdd,0xdc),
                    listOf(0x14,0x41,0x53,0x44,0x46,0x47,0x48,0x4a,0x4b,0x4c,0xba,0xde,0x0d),
                    listOf(0x5a,0x58,0x43,0x56,0x42,0x4e,0x4d,0xbc,0xbe,0xbf,0x20),
                    listOf(0x2c,0x91,0x13,0x2d,0x24,0x21,0x2e,0x23,0x22,0x25,0x26,0x28,0x27),
                )
                rows.forEach { row ->
                    Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(3.dp)) {
                        row.forEach { code ->
                            Button(onClick={coordinator.sendShortcut(InputShortcut("keyboard",InputKeys.catalog[code] ?: "Key",code,ctrl,alt,shift,meta))},enabled=enabled,
                                modifier=Modifier.weight(1f).height(32.dp).semantics {contentDescription=InputKeys.catalog[code] ?: ""}, contentPadding=PaddingValues(0.dp),shape=MaterialTheme.shapes.small) {
                                Text(keyboardLabel(code),style=MaterialTheme.typography.labelSmall,maxLines=1)
                            }
                        }
                    }
                }
            }
        }
    }
}

private fun keyboardLabel(code:Int):String=when(code) {
    0x08->"⌫";0x14->"Caps";0x2c->"PrtSc";0x91->"ScrLk"
    0x2d->"Ins";0x2e->"Del";0x21->"PgUp";0x22->"PgDn"
    else->InputKeys.catalog[code] ?: ""
}
