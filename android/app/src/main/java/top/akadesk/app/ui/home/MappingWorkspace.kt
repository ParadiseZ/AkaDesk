package top.akadesk.app.ui.home

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.*
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.DeleteOutline
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import top.akadesk.app.controller.*
import java.util.UUID

@Composable
internal fun MappingProfiles(c:ControllerCoordinator,onClose:()->Unit,onEdit:()->Unit) {
    val context=LocalContext.current;val scope=rememberCoroutineScope()
    var error by remember {mutableStateOf<String?>(null)}
    var name by remember {mutableStateOf("")}
    var rename by remember {mutableStateOf<String?>(null)}
    var creating by remember {mutableStateOf(false)}
    var deleting by remember {mutableStateOf<String?>(null)}
    var export by remember {mutableStateOf("")}
    var busy by remember {mutableStateOf(false)}
    val importer=rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) {uri->if(uri!=null) scope.launch {
        busy=true
        try {
            val raw=withContext(Dispatchers.IO) {context.contentResolver.openInputStream(uri)?.use {input->
                val output=java.io.ByteArrayOutputStream();val buffer=ByteArray(4096)
                while(output.size()<=65536) {val read=input.read(buffer,0,minOf(buffer.size,65537-output.size()));if(read<0) break;output.write(buffer,0,read)}
                require(output.size()<=65536);output.toString("UTF-8")
            } ?: error("无法读取文件")}
            c.importInputProfile(raw);error=null
        } catch(_:Exception) {error="导入失败，请检查方案格式、大小和方案数量"} finally {busy=false}
    }}
    val exporter=rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/json")) {uri->if(uri!=null) scope.launch {
        busy=true
        try {withContext(Dispatchers.IO) {context.contentResolver.openOutputStream(uri,"w")?.use {it.write(export.toByteArray())} ?: error("无法保存")};error=null}
        catch(_:Exception) {error="导出失败，请重新选择保存位置"} finally {busy=false}
    }}
    BackHandler(onBack=onClose)
    Surface(Modifier.fillMaxSize().testTag("mapping-profiles")) {
        Column(Modifier.padding(16.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                TextButton(onClick=onClose) {Text("关闭")};Text("操控方案",Modifier.weight(1f),style=MaterialTheme.typography.titleLarge)
                TextButton(enabled=!busy,onClick={importer.launch(arrayOf("application/json","text/plain","application/octet-stream"))}) {Text("导入")}
                Button(enabled=!busy,onClick={creating=true;name=""}) {Text("新建")}
            }
            Text("我的方案",style=MaterialTheme.typography.titleMedium)
            c.inputProfileNames.forEach {profile->
                RemoteCard {
                    Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                        FilterChip(selected=c.selectedInputProfile==profile,onClick={c.selectInputProfile(profile)},label={Text(profile)},modifier=Modifier.weight(1f))
                        TextButton(onClick={c.selectInputProfile(profile);onEdit()}) {Text("编辑")}
                        var menu by remember(profile) {mutableStateOf(false)}
                        Box {
                            TextButton(onClick={menu=true}) {Text("⋮")}
                            DropdownMenu(menu,onDismissRequest={menu=false}) {
                                DropdownMenuItem(text={Text("重命名")},onClick={menu=false;rename=profile;name=profile})
                                DropdownMenuItem(text={Text("复制")},enabled=!busy,onClick={menu=false;runCatching {c.copyInputProfile(profile)}.onFailure {error="复制失败或方案已达到上限"}})
                                DropdownMenuItem(text={Text("导出")},enabled=!busy,onClick={menu=false;export=c.exportInputProfile(profile);exporter.launch("$profile.akadesk.json")})
                                DropdownMenuItem(text={Text("删除")},enabled=c.inputProfileNames.size>1,onClick={menu=false;deleting=profile})
                            }
                        }
                    }
                }
            }
            Text("本地预设",style=MaterialTheme.typography.titleMedium)
            Row(horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                TextButton(onClick={if(c.createInputProfile("键盘预设 ${System.currentTimeMillis()%100000}")) {c.saveInputShortcuts(InputKeys.defaults());onEdit()} else error="无法新建方案"}) {Text("常用键盘")}
                TextButton(onClick={if(c.createInputProfile("方向预设 ${System.currentTimeMillis()%100000}")) {c.saveInputShortcuts(listOf(0x57,0x41,0x53,0x44).map {InputShortcut(UUID.randomUUID().toString(),InputKeys.catalog[it]!!,it)});onEdit()} else error="无法新建方案"}) {Text("WASD 方向键")}
            }
            error?.let {Text(it,color=MaterialTheme.colorScheme.error)}
        }
    }
    if(creating || rename!=null) AlertDialog(onDismissRequest={creating=false;rename=null},title={Text(if(creating) "新建方案" else "重命名方案")},text={OutlinedTextField(name,onValueChange={name=it.take(32)},label={Text("方案名称")},singleLine=true)},
        confirmButton={TextButton(onClick={val ok=if(creating) c.createInputProfile(name) else c.renameInputProfile(rename!!,name);if(ok) {val edit=creating;creating=false;rename=null;error=null;if(edit) onEdit()} else error="名称无效、重复或方案数量已达上限"}) {Text("确定")}},
        dismissButton={TextButton(onClick={creating=false;rename=null}) {Text("取消")}})
    deleting?.let {profile->AlertDialog(onDismissRequest={deleting=null},title={Text("删除方案？")},text={Text("删除“$profile”及其按键、位置和视角配置")},confirmButton={TextButton(onClick={if(!c.deleteInputProfile(profile)) error="删除失败";deleting=null}) {Text("删除")}},dismissButton={TextButton(onClick={deleting=null}) {Text("取消")}})}
}

@Composable
internal fun MappingEditorSurface(initial:MappingDraft,defaultSize:Int,defaultOpacity:Float,onSave:(MappingDraft)->Boolean,onClose:()->Unit) {
    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides 32.dp) {
    var draft by remember(initial.originalName) {mutableStateOf(initial)}
    var selected by remember {mutableStateOf<String?>(null)}
    var picker by remember {mutableStateOf<Int?>(null)}
    var pickerActions by remember {mutableStateOf<List<MappingAction>>(emptyList())}
    var view by remember {mutableStateOf(false)}
    var exit by remember {mutableStateOf(false)}
    var clear by remember {mutableStateOf(false)}
    var naming by remember {mutableStateOf(false)}
    var error by remember {mutableStateOf<String?>(null)}
    var area by remember {mutableStateOf(IntSize.Zero)}
    val density=LocalDensity.current
    val binding=draft.bindings.firstOrNull {it.id==selected}
    fun change(value:InputShortcut) {draft=draft.copy(bindings=draft.bindings.map {if(it.id==value.id) value else it})}
    fun leave() {if(draft!=initial) exit=true else onClose()}
    fun newPicker(combo:Boolean) {if(draft.bindings.size>=30) {error="最多保存 30 个映射";return};selected=null;pickerActions=emptyList();picker=if(combo) 2 else 1}
    BackHandler {when {picker!=null->picker=null;view->view=false;else->leave()}}
    Box(Modifier.fillMaxSize().onSizeChanged {area=it}.testTag("mapping-editor")) {
        draft.bindings.forEachIndexed {index,key->
            val position=draft.positions[key.id] ?: (.1f+(index%5)*.17f to .45f+(index/5)*.09f)
            val size=(key.size ?: defaultSize).dp;val px=with(density) {size.toPx()}
            val availableW=(area.width-px).coerceAtLeast(1f);val availableH=(area.height-px).coerceAtLeast(1f)
            Box(Modifier.offset {IntOffset((position.first*availableW).toInt(),(position.second*availableH).toInt())}.size(size).clip(CircleShape)
                .background(Color(0xFF303744).copy(alpha=key.opacity ?: defaultOpacity)).border(if(selected==key.id) 2.dp else 1.dp,if(selected==key.id) Color(0xFF477CFF) else Color.LightGray,CircleShape)
                .testTag("mapping-edit-${key.id}").clickable {selected=key.id}
                .pointerInput(key.id,area,size) {detectDragGestures(onDragStart={selected=key.id}) {point,delta->
                    point.consume();val current=draft.positions[key.id] ?: position
                    draft=draft.copy(positions=draft.positions+(key.id to ((current.first+delta.x/availableW).coerceIn(0f,1f) to (current.second+delta.y/availableH).coerceIn(0f,1f))))
                }},contentAlignment=Alignment.Center) {MappingKeyLabel(key.label,key.size ?: defaultSize)}
        }
        Surface(Modifier.align(Alignment.TopCenter).displayCutoutPadding().widthIn(max=720.dp).fillMaxWidth().testTag("mapping-properties"),color=Color(0xD9262930),shape=RoundedCornerShape(bottomStart=6.dp,bottomEnd=6.dp)) {
            Column(Modifier.padding(4.dp)) {
                Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                    Row(Modifier.weight(1f).horizontalScroll(rememberScrollState()),verticalAlignment=Alignment.CenterVertically) {
                        MappingTextAction("${draft.name} ✎",onClick={naming=true})
                        MappingTextAction("＋ 单个按键",onClick={newPicker(false)})
                        MappingTextAction("＋ 组合键",onClick={newPicker(true)})
                        MappingTextAction("3D 视角控制",onClick={view=true})
                        MappingTextAction("清空",onClick={clear=true})
                    }
                    MappingTextAction("取消",onClick=::leave)
                    Button(onClick={if(onSave(draft)) onClose() else error="保存失败，请检查名称、按键与视角配置"},
                        modifier=Modifier.height(32.dp),shape=RoundedCornerShape(6.dp),contentPadding=PaddingValues(horizontal=10.dp,vertical=0.dp)) {Text("保存方案",fontSize=12.sp)}
                }
                if(binding!=null) Column(Modifier.heightIn(max=88.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(2.dp)) {
                    Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                        Row(Modifier.weight(1f).horizontalScroll(rememberScrollState()),verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(4.dp)) {
                            BasicTextField(binding.label,onValueChange={change(binding.copy(label=it.take(32)))},singleLine=true,
                                textStyle=MaterialTheme.typography.bodySmall.copy(color=Color.White,fontSize=12.sp),cursorBrush=SolidColor(MaterialTheme.colorScheme.primary),
                                modifier=Modifier.width(148.dp).height(32.dp).border(1.dp,Color.Gray,RoundedCornerShape(4.dp)).padding(horizontal=6.dp).testTag("mapping-key-name"),
                                decorationBox={field->Row(verticalAlignment=Alignment.CenterVertically) {Text("名称",fontSize=11.sp,color=Color.LightGray,modifier=Modifier.padding(end=6.dp));Box(Modifier.weight(1f)) {field()}}})
                            MappingTextAction(binding.chordLabel(),onClick={pickerActions=binding.effectiveActions();picker=if(pickerActions.size>1) 2 else 1})
                            MappingBehavior.entries.forEach {mode->MappingOption(mode.title,binding.behavior==mode,
                                enabled=mode!=MappingBehavior.Toggle || binding.effectiveActions().none {it.kind==MappingActionKind.Wheel},
                                onClick={change(binding.copy(behavior=mode))})}
                            MappingOption("可移动",binding.movable,onClick={change(binding.copy(movable=!binding.movable))})
                        }
                        IconButton(onClick={draft=draft.copy(bindings=draft.bindings.filterNot {it.id==binding.id},positions=draft.positions-binding.id);selected=null},modifier=Modifier.size(32.dp)) {Icon(Icons.Outlined.DeleteOutline,"删除按键",Modifier.size(18.dp),tint=MaterialTheme.colorScheme.error)}
                        IconButton(onClick={selected=null},modifier=Modifier.size(32.dp)) {Icon(Icons.Outlined.Close,"收起属性",Modifier.size(18.dp))}
                    }
                    Row(Modifier.fillMaxWidth().testTag("mapping-sliders"),verticalAlignment=Alignment.CenterVertically) {
                        Text("大小 ${binding.size ?: defaultSize}",Modifier.width(52.dp),fontSize=11.sp)
                        CompactRemoteSlider((binding.size ?: defaultSize).toFloat(),onValueChange={change(binding.copy(size=it.toInt()))},valueRange=16f..100f,modifier=Modifier.weight(1f))
                        Text("不透明度 ${((binding.opacity ?: defaultOpacity)*100).toInt()}%",Modifier.padding(start=8.dp).width(92.dp),fontSize=11.sp)
                        CompactRemoteSlider(binding.opacity ?: defaultOpacity,onValueChange={change(binding.copy(opacity=it))},valueRange=.1f..1f,modifier=Modifier.weight(1f))
                    }
                }
                error?.let {Text(it,color=MaterialTheme.colorScheme.error)}
            }
        }
        if(picker!=null) MappingActionPicker(picker==2,pickerActions,onClose={picker=null}) {actions->
            val existing=draft.bindings.firstOrNull {it.id==selected}
            val value=(existing ?: InputShortcut(UUID.randomUUID().toString(),actions.joinToString("+") {it.label()}.take(32),0x0d)).copy(actions=actions,
                behavior=if(actions.any {it.kind==MappingActionKind.Wheel}) MappingBehavior.Tap else existing?.behavior ?: MappingBehavior.Hold)
            if(existing==null) draft=draft.copy(bindings=draft.bindings+value,positions=draft.positions+(value.id to (.5f to .5f))) else change(value)
            selected=value.id;picker=null
        }
        if(view) MappingViewEditor(draft.view,onClose={view=false},onChange={draft=draft.copy(view=it)})
    }
    if(exit) AlertDialog(onDismissRequest={exit=false},title={Text("退出编辑？")},text={Text("尚未保存的按键、位置和配置将被丢弃")},confirmButton={TextButton(onClick=onClose) {Text("放弃更改")}},dismissButton={TextButton(onClick={exit=false}) {Text("继续编辑")}})
    if(clear) AlertDialog(onDismissRequest={clear=false},title={Text("清空当前草稿？")},confirmButton={TextButton(onClick={draft=draft.copy(bindings=emptyList(),positions=emptyMap());selected=null;clear=false}) {Text("清空")}},dismissButton={TextButton(onClick={clear=false}) {Text("取消")}})
    if(naming) AlertDialog(onDismissRequest={naming=false},title={Text("方案名称")},text={OutlinedTextField(draft.name,onValueChange={draft=draft.copy(name=it.take(32))},singleLine=true)},confirmButton={TextButton(onClick={naming=false}) {Text("确定")}})
    }
}

@Composable private fun MappingTextAction(title:String,onClick:()->Unit) {
    TextButton(onClick=onClick,modifier=Modifier.height(32.dp),contentPadding=PaddingValues(horizontal=8.dp,vertical=0.dp)) {Text(title,fontSize=12.sp,maxLines=1)}
}

@Composable private fun MappingOption(title:String,selected:Boolean,enabled:Boolean=true,onClick:()->Unit) {
    Surface(shape=RoundedCornerShape(4.dp),color=if(selected) MaterialTheme.colorScheme.primary.copy(alpha=.35f) else Color.Transparent) {
        Box(Modifier.height(28.dp).clickable(enabled=enabled,onClick=onClick).padding(horizontal=8.dp),contentAlignment=Alignment.Center) {
            Text(title,fontSize=11.sp,color=if(enabled) Color.White else Color.Gray)
        }
    }
}

@Composable private fun MappingActionPicker(combo:Boolean,initial:List<MappingAction>,onClose:()->Unit,onSave:(List<MappingAction>)->Unit) {
    var actions by remember {mutableStateOf(initial)};var category by remember {mutableIntStateOf(0)}
    Surface(Modifier.fillMaxSize().testTag("mapping-key-picker")) {
        Column(Modifier.fillMaxSize().padding(12.dp),verticalArrangement=Arrangement.spacedBy(6.dp)) {
            Row(verticalAlignment=Alignment.CenterVertically) {TextButton(onClick=onClose) {Text("返回编辑")};Text(if(combo) "选择 2–4 个键盘或鼠标动作" else "选择一个按键",Modifier.weight(1f));if(combo) Button(enabled=actions.size in 2..4,onClick={onSave(actions)}) {Text("确认组合键")}}
            if(combo) Row(Modifier.horizontalScroll(rememberScrollState())) {actions.forEach {action->InputChip(selected=true,onClick={actions=actions-action},label={Text("${action.label()} ×")})}}
            if(actions.size>4) Text("原组合超过四个动作，请移除动作后确认",color=MaterialTheme.colorScheme.error)
            Row {listOf("键盘","鼠标","功能键").forEachIndexed {index,label->FilterChip(category==index,onClick={category=index},label={Text(label)})}}
            val choices=when(category) {
                1->(1..3).map {MappingAction(MappingActionKind.Mouse,it)}+listOf(MappingAction(MappingActionKind.Wheel,1),MappingAction(MappingActionKind.Wheel,-1))
                2->InputKeys.catalog.keys.filter {it in 0x70..0x87 || it<0x30}.map {MappingAction(MappingActionKind.Key,it)}
                else->InputKeys.catalog.keys.map {MappingAction(MappingActionKind.Key,it)}
            }
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
            choices.chunked(12).forEach {row->Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(4.dp)) {row.forEach {action->
                OutlinedButton(enabled=!combo || action in actions || actions.size<4,onClick={if(!combo) onSave(listOf(action)) else actions=if(action in actions) actions-action else actions+action},modifier=Modifier.weight(1f),contentPadding=PaddingValues(2.dp)) {Text(action.label(),maxLines=2,style=MaterialTheme.typography.labelSmall)}
            }}}
            }
        }
    }
}

@Composable private fun MappingViewEditor(value:MappingViewControl,onClose:()->Unit,onChange:(MappingViewControl)->Unit) {
    Surface(Modifier.fillMaxSize()) {Column(Modifier.padding(16.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment=Alignment.CenterVertically) {TextButton(onClick=onClose) {Text("返回编辑")};Text("3D 视角控制",style=MaterialTheme.typography.titleLarge)}
        ToggleRow("开启 3D 视角控制",value.enabled) {onChange(value.copy(enabled=it))}
        Row(Modifier.horizontalScroll(rememberScrollState())) {ViewTrigger.entries.forEach {trigger->FilterChip(selected=value.trigger==trigger,onClick={onChange(value.copy(trigger=trigger))},label={Text(trigger.title)})}}
        ToggleRow("仅右半屏控制",value.rightHalf) {onChange(value.copy(rightHalf=it))}
        Text("X 轴灵敏度：${value.sensitivityX}");Slider(value.sensitivityX,onValueChange={onChange(value.copy(sensitivityX=it))},valueRange=.1f..3f)
        Text("Y 轴灵敏度：${value.sensitivityY}");Slider(value.sensitivityY,onValueChange={onChange(value.copy(sensitivityY=it))},valueRange=.1f..3f)
        ToggleRow("拖动视角时隐藏未按下的映射按键",value.hideButtons) {onChange(value.copy(hideButtons=it))}
        ToggleRow("关闭画面双击",value.disableDoubleTap) {onChange(value.copy(disableDoubleTap=it))}
        Text("通过相对鼠标移动控制视角；游戏需支持相对鼠标输入。配置随当前方案保存，编辑期间不向电脑发送操作。",style=MaterialTheme.typography.bodySmall)
    }}
}
