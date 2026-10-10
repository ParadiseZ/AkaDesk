@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
package top.akadesk.app.ui.home

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import top.akadesk.app.controller.*
import java.util.UUID

@Composable
fun InputMappingEditor(coordinator: ControllerCoordinator, onBack: () -> Unit) {
    var editing by remember { mutableStateOf<InputShortcut?>(null) }
    var label by rememberSaveable { mutableStateOf("") }
    var key by rememberSaveable { mutableIntStateOf(0x0d) }
    var ctrl by rememberSaveable { mutableStateOf(false) }
    var alt by rememberSaveable { mutableStateOf(false) }
    var shift by rememberSaveable { mutableStateOf(false) }
    var meta by rememberSaveable { mutableStateOf(false) }
    var menu by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    fun resetEditor() {
        editing = null
        label = ""
        key = 0x0d
        ctrl = false
        alt = false
        shift = false
        meta = false
        error = null
    }
    BackHandler(onBack=onBack)
    Column(Modifier.fillMaxSize().padding(16.dp).verticalScroll(rememberScrollState()), verticalArrangement=Arrangement.spacedBy(8.dp)) {
        Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) {
            TextButton(onClick=onBack) { Text("返回") }
            Text("按键映射", style=MaterialTheme.typography.titleLarge)
            TextButton(onClick={
                if (coordinator.saveInputShortcuts(InputKeys.defaults())) resetEditor()
                else error="保存失败，请重试"
            }) { Text("重置") }
        }
        Text("保存常用按键或组合键，在远程键盘中点按发送。最多 30 个。")
        coordinator.inputShortcuts.forEach { value ->
            Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) {
                TextButton(onClick={ editing=value; label=value.label; key=value.key; ctrl=value.ctrl; alt=value.alt; shift=value.shift; meta=value.meta; error=null }) {
                    Text("${value.label} · ${value.chordLabel()}")
                }
                TextButton(onClick={
                    if (coordinator.saveInputShortcuts(coordinator.inputShortcuts.filterNot {it.id==value.id})) {
                        if (editing?.id == value.id) resetEditor()
                    } else error="保存失败，请重试"
                }) { Text("删除") }
            }
        }
        HorizontalDivider()
        Text(if(editing==null) "新增映射" else "编辑映射")
        OutlinedTextField(label={Text("名称")},value=label,onValueChange={label=it.take(32)},modifier=Modifier.fillMaxWidth(),singleLine=true)
        Box {
            OutlinedButton(onClick={menu=true}) { Text("按键：${InputKeys.catalog[key]}") }
            DropdownMenu(expanded=menu,onDismissRequest={menu=false}) {
                InputKeys.catalog.forEach { (code,name) -> DropdownMenuItem(text={Text(name)},onClick={key=code; menu=false}) }
            }
        }
        Row(Modifier.horizontalScroll(rememberScrollState())) {
            FilterChip(selected=ctrl,onClick={ctrl=!ctrl},label={Text("Ctrl")})
            Spacer(Modifier.width(8.dp)); FilterChip(selected=alt,onClick={alt=!alt},label={Text("Alt")})
            Spacer(Modifier.width(8.dp)); FilterChip(selected=shift,onClick={shift=!shift},label={Text("Shift")})
            Spacer(Modifier.width(8.dp)); FilterChip(selected=meta,onClick={meta=!meta},label={Text("Win")})
        }
        Row {
            Button(enabled=label.isNotBlank(),onClick={
                val next=InputShortcut(editing?.id ?: UUID.randomUUID().toString(),label.trim(),key,ctrl,alt,shift,meta)
                val values = if (editing == null) coordinator.inputShortcuts + next
                    else coordinator.inputShortcuts.map { if (it.id == next.id) next else it }
                if(coordinator.saveInputShortcuts(values)) resetEditor()
                else error=if(values.size>30) "最多保存 30 个映射" else "保存失败，请重试"
            }) { Text("保存") }
            TextButton(onClick=::resetEditor) { Text("新增") }
        }
        error?.let { Text(it,color=MaterialTheme.colorScheme.error) }
    }
}

@Composable
fun RemoteKeyboardSheet(coordinator: ControllerCoordinator, onDismiss: () -> Unit) {
    var text by rememberSaveable { mutableStateOf("") }
    var editor by rememberSaveable { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    DisposableEffect(coordinator) {
        coordinator.editingLocalInput=true
        coordinator.releaseRemoteInput()
        onDispose { coordinator.editingLocalInput=false; coordinator.releaseRemoteInput() }
    }
    ModalBottomSheet(onDismissRequest=onDismiss) {
        if(editor) InputMappingEditor(coordinator,onBack={editor=false})
        else Column(Modifier.padding(16.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) {
                Text("电脑键盘",style=MaterialTheme.typography.titleLarge)
                TextButton(onClick={editor=true}) {Text("编辑映射")}
            }
            OutlinedTextField(value=text,onValueChange={text=it},label={Text("输入文字")},modifier=Modifier.fillMaxWidth(),maxLines=3)
            Button(enabled=coordinator.remoteInputReady && text.isNotEmpty() && text.toByteArray().size<=4096 && '\u0000' !in text,
                onClick={ if(coordinator.sendText(text)) {text="";error=null} else error="文字发送失败，请重试" }) {Text("发送文字")}
            if(text.toByteArray().size>4096) Text("文字过长，请分段发送",color=MaterialTheme.colorScheme.error)
            coordinator.inputShortcuts.chunked(4).forEach { group ->
                Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(6.dp)) {
                    group.forEach { value -> OutlinedButton(modifier=Modifier.weight(1f),enabled=coordinator.remoteInputReady,onClick={coordinator.sendShortcut(value)}) {Text(value.label)} }
                }
            }
            Row(Modifier.horizontalScroll(rememberScrollState())) {
                TextButton(onClick={coordinator.scrollInput(0f,1f)},enabled=coordinator.remoteInputReady) {Text("滚轮 ↑")}
                TextButton(onClick={coordinator.scrollInput(0f,-1f)},enabled=coordinator.remoteInputReady) {Text("滚轮 ↓")}
                TextButton(onClick={coordinator.scrollInput(-1f,0f)},enabled=coordinator.remoteInputReady) {Text("横滚 ←")}
                TextButton(onClick={coordinator.scrollInput(1f,0f)},enabled=coordinator.remoteInputReady) {Text("横滚 →")}
            }
            error?.let {Text(it,color=MaterialTheme.colorScheme.error)}
            Text("点按或拖动电脑画面操作左键；右键从底部工具栏发送。",style=MaterialTheme.typography.bodySmall)
        }
    }
}
