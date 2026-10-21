@file:OptIn(androidx.compose.ui.ExperimentalComposeUiApi::class)
package top.akadesk.app.ui.home

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.*
import kotlinx.coroutines.delay
import top.akadesk.app.MainActivity
import top.akadesk.app.controller.*

private enum class RemotePanel(val title:String) { Operation("操作"), Display("显示"), Security("安全"), Peripherals("外设"), More("更多"), Network("网络状态"), HideToolbar("自动隐藏工具栏"), Pip("小窗模式"), Guide("操控指南"), MappingOptions("映射设置"), System("系统快捷操作"), Advanced("高级选项") }

@Composable
fun RemoteSessionScreen(coordinator:ControllerCoordinator, session:ActiveRemoteSession, onDisconnect:()->Unit) {
    val typography=MaterialTheme.typography
    MaterialTheme(colorScheme=darkColorScheme(primary=Color(0xFF477CFF),onPrimary=Color.White,surface=Color(0xFF272930),surfaceVariant=Color(0xFF42454E),onSurface=Color.White,onSurfaceVariant=Color.White),
        typography=typography.copy(bodyLarge=typography.bodyLarge.copy(fontSize=13.sp,lineHeight=18.sp),
            bodyMedium=typography.bodyMedium.copy(fontSize=12.sp,lineHeight=16.sp),
            labelLarge=typography.labelLarge.copy(fontSize=12.sp,lineHeight=16.sp),
            labelSmall=typography.labelSmall.copy(fontSize=10.sp,lineHeight=12.sp),
            titleLarge=typography.titleLarge.copy(fontSize=16.sp,lineHeight=20.sp))) {
        CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides 32.dp) {
        Surface(Modifier.fillMaxSize(),color=Color.Black,contentColor=Color.White) {RemoteSessionContent(coordinator,session,onDisconnect)}
        }
    }
}

@Composable
private fun RemoteSessionContent(c:ControllerCoordinator,s:ActiveRemoteSession,onDisconnect:()->Unit) {
    val prefs=c.remotePreferences
    var panel by rememberSaveable(s.sessionId) { mutableStateOf<RemotePanel?>(null) }
    var keyboard by rememberSaveable(s.sessionId) { mutableStateOf(false) }
    var toolbar by rememberSaveable(s.sessionId) { mutableStateOf(true) }
    var mappingEditing by rememberSaveable(s.sessionId) {mutableStateOf(false)}
    var mappingProfiles by rememberSaveable(s.sessionId) {mutableStateOf(false)}
    var returnToProfiles by rememberSaveable(s.sessionId) {mutableStateOf(false)}
    var guideStep by rememberSaveable { mutableIntStateOf(0) }
    var error by remember { mutableStateOf<String?>(null) }
    var network by remember { mutableStateOf(RemoteNetworkStats()) }
    var files by rememberSaveable(s.sessionId) { mutableStateOf(false) }
    val context=LocalContext.current
    val pip=c.pictureInPicture
    DisposableEffect(context,s.sessionId) {
        val activity=context as? android.app.Activity
        val previous=activity?.volumeControlStream
        activity?.volumeControlStream=android.media.AudioManager.STREAM_MUSIC
        onDispose {if(previous!=null) activity.volumeControlStream=previous}
    }
    val ready=c.hasActiveRemoteVideoTrack(s.sessionId)
    val stalled=c.isActiveRemoteVideoStalled(s.sessionId)
    val enabled=c.remoteInputReady && ready && !stalled && !c.viewOnly && s.stateLabel!="DISCONNECTING" && !pip
    val blocked=panel!=null || keyboard || files || mappingEditing || mappingProfiles
    ClipboardSync(c)
    DisposableEffect(c,s.sessionId) {onDispose {c.editingLocalInput=false;c.releaseRemoteInput()}}
    LaunchedEffect(blocked,enabled) {c.editingLocalInput=blocked;c.releaseRemoteInput()}
    LaunchedEffect(toolbar,panel,keyboard,prefs.toolbarHide,prefs.mappingEnabled,pip) {
        if(toolbar && panel==null && !keyboard && !pip && (prefs.toolbarHide==ToolbarHide.Always || prefs.toolbarHide==ToolbarHide.Mapping && prefs.mappingEnabled)) {delay(3000);toolbar=false}
    }
    LaunchedEffect(panel) {if(panel==RemotePanel.Display) c.refreshDisplaySettings()}
    LaunchedEffect(s.sessionId) {
        val sampler=RemoteStatsSampler()
        while(true) {
            if(c.toolsReady) runCatching {c.applyConfirmedVideoStatus(c.toolRequest("status"))}
            c.collectRemoteMediaStats {report -> android.os.Handler(android.os.Looper.getMainLooper()).post {if(c.activeSession?.sessionId==s.sessionId) network=sampler.sample(report)}}
            delay(2000)
        }
    }
    BackHandler {when {files->files=false; keyboard->keyboard=false; panel!=null->panel=if(panel==RemotePanel.Operation) null else RemotePanel.Operation; else->onDisconnect()}}
    // Video and mappings share the full viewport; only the controls avoid the cutout.
    val controlInsets=if(prefs.avoidCutout && !pip) Modifier.windowInsetsPadding(WindowInsets.displayCutout.only(WindowInsetsSides.End + WindowInsetsSides.Vertical)) else Modifier
    Box(Modifier.fillMaxSize().background(Color.Black).testTag("remote-viewport")) {
            BoxWithConstraints(Modifier.fillMaxSize()) {
                RemoteVideoSurface(c,s,enabled && !blocked, onReveal={toolbar=true}, rightModeEnabled=prefs.pointerMode==PointerMode.Trackpad)
                if(!pip && prefs.mappingEnabled && !keyboard && !files && !mappingEditing && !mappingProfiles) MappingOverlay(c,enabled && !blocked,false)
                if(!pip && prefs.virtualMouse && !blocked && !c.viewOnly) VirtualMouse(c,enabled)
                if(!pip && (prefs.showNetwork || !ready || stalled)) Text(
                    if(stalled) "画面中断，等待恢复…" else if(!ready) "正在连接电脑画面…" else "${metric(network.rttMs)} ms · ${metric(network.fps)} FPS · ${metric(network.mbps)} Mbps",
                    color=Color.White,modifier=Modifier.align(Alignment.TopStart).background(Color(0x99000000),RoundedCornerShape(6.dp)).padding(8.dp),style=MaterialTheme.typography.labelSmall)
                if(c.viewOnly && !pip) Text("仅观看",color=Color.White,modifier=Modifier.align(Alignment.TopCenter).padding(8.dp))
                if(!toolbar && !pip && panel==null) TextButton(onClick={toolbar=true},modifier=Modifier.align(Alignment.CenterEnd)) {Text("操作")}
                if(!pip && panel!=null) {
                    Box(Modifier.fillMaxSize().clickable(indication=null,interactionSource=remember {androidx.compose.foundation.interaction.MutableInteractionSource()}) {panel=null})
                    Surface(Modifier.align(Alignment.CenterEnd).then(controlInsets).padding(end=48.dp).fillMaxHeight().width(minOf(272.dp,maxWidth*.42f)).testTag("remote-panel"),color=Color(0xEF292C34),contentColor=Color.White) {
                        Column {
                            if(panel!=RemotePanel.Operation) Row(Modifier.fillMaxWidth().height(36.dp),verticalAlignment=Alignment.CenterVertically) {
                                IconButton(onClick={panel=RemotePanel.Operation},modifier=Modifier.size(32.dp)) {Icon(Icons.Outlined.ChevronLeft,"返回")}
                                Text(panel!!.title,modifier=Modifier.weight(1f),fontWeight=FontWeight.Bold)
                                IconButton(onClick={panel=null},modifier=Modifier.size(32.dp)) {Icon(Icons.Outlined.Close,"关闭面板")}
                            }
                            when(panel) {
                                RemotePanel.Operation->Column(Modifier.verticalScroll(rememberScrollState()).padding(6.dp),verticalArrangement=Arrangement.spacedBy(6.dp)) {
                                    Row(Modifier.horizontalScroll(rememberScrollState()),horizontalArrangement=Arrangement.spacedBy(6.dp)) {
                                        QuickTile("退出远控",Icons.Outlined.Logout,onClick=onDisconnect)
                                        QuickTile("旋转屏幕",Icons.Outlined.ScreenRotation,onClick={c.remotePortrait=!c.remotePortrait})
                                        QuickTile("声音",Icons.Outlined.VolumeUp,!c.phoneAudioMuted,c.remoteAudioReady,onClick=c::togglePhoneAudioMute)
                                        QuickTile("仅观看",Icons.Outlined.Visibility,c.viewOnly,!c.viewOnlyPending,onClick={c.updateViewOnly(!c.viewOnly)})
                                        if(prefs.showPipShortcut) QuickTile("小窗",Icons.Outlined.PictureInPicture,onClick={if((context as? MainActivity)?.enterRemotePip()!=true) error="未能进入小窗，请检查系统权限"})
                                    }
                                    RemoteCard {
                                        ToggleRow("启用按键映射",prefs.mappingEnabled,enabled) {c.updateRemotePreferences(prefs.copy(mappingEnabled=it))}
                                        if(prefs.mappingEnabled) {
                                            MenuRow("切换操控方案",c.selectedInputProfile) {panel=null;mappingProfiles=true}
                                            MenuRow("编辑当前方案") {keyboard=false;panel=null;returnToProfiles=false;mappingEditing=true}
                                            MenuRow("调整按钮位置") {panel=null;returnToProfiles=false;mappingEditing=true}
                                            MenuRow("其他设置") {panel=RemotePanel.MappingOptions}
                                        }
                                    }
                                    RemoteCard {
                                        Row(Modifier.fillMaxWidth()) {
                                            listOf(PointerMode.Touch to "屏幕触控",PointerMode.Trackpad to "鼠标指针").forEach {(mode,title)->
                                                Button(onClick={c.updateRemotePreferences(prefs.copy(pointerMode=mode))},modifier=Modifier.weight(1f),colors=ButtonDefaults.buttonColors(containerColor=if(prefs.pointerMode==mode) MaterialTheme.colorScheme.primary else Color(0xFF33363E)),shape=RoundedCornerShape(6.dp),contentPadding=PaddingValues(8.dp)) {Text(title)}
                                            }
                                        }
                                        ToggleRow(if(prefs.pointerMode==PointerMode.Trackpad) "显示鼠标按键" else "虚拟鼠标",prefs.virtualMouse) {c.updateRemotePreferences(prefs.copy(virtualMouse=it))}
                                        if(prefs.pointerMode==PointerMode.Touch) ToggleRow("平板触控模式",prefs.tabletTouch,c.toolsReady) {c.updateRemotePreferences(prefs.copy(tabletTouch=it))}
                                    }
                                    RemoteCard {
                                        MenuRow("显示",icon=Icons.Outlined.Monitor) {panel=RemotePanel.Display}
                                        MenuRow("安全",icon=Icons.Outlined.Security) {panel=RemotePanel.Security}
                                        MenuRow("外设",icon=Icons.Outlined.Usb) {panel=RemotePanel.Peripherals}
                                        MenuRow("更多",icon=Icons.Outlined.Apps) {panel=RemotePanel.More}
                                    }
                                    RemoteCard {MenuRow("系统快捷操作",icon=Icons.Outlined.Computer) {panel=RemotePanel.System}}
                                }
                                RemotePanel.Display->Column(Modifier.verticalScroll(rememberScrollState())) {VideoQualityControls(c);DisplaySettingsContent(c,scrollable=false);ReservedFeature("虚拟显示器", "安装虚拟显示驱动后可启用")}
                                RemotePanel.MappingOptions->Column(Modifier.padding(12.dp).verticalScroll(rememberScrollState())) {
                                    Text("默认按钮大小：${prefs.buttonSize}")
                                    CompactRemoteSlider(value=prefs.buttonSize.toFloat(),onValueChange={c.updateRemotePreferences(prefs.copy(buttonSize=it.toInt()))},valueRange=24f..72f)
                                    Text("默认按钮不透明度")
                                    CompactRemoteSlider(value=prefs.buttonOpacity,onValueChange={c.updateRemotePreferences(prefs.copy(buttonOpacity=it))},valueRange=.2f..1f)
                                    ToggleRow("双指缩放画面",prefs.pinchZoom) {c.updateRemotePreferences(prefs.copy(pinchZoom=it))}
                                    ToggleRow("按键振动反馈",prefs.buttonHaptics) {c.updateRemotePreferences(prefs.copy(buttonHaptics=it))}
                                    Text("按键单独设置的大小与不透明度优先于默认值",style=MaterialTheme.typography.bodySmall)
                                }
                                RemotePanel.Security->RemoteSecurityPanel(c)
                                RemotePanel.Peripherals->PeripheralPanel()
                                RemotePanel.More->Column(Modifier.padding(12.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(12.dp)) {
                                    RemoteCard {MenuRow("文件传输",icon=Icons.Outlined.FolderOpen) {files=true;panel=null}}
                                    RemoteCard {MenuRow("网络状态","${metric(network.rttMs)} ms",Icons.Outlined.NetworkCheck) {panel=RemotePanel.Network}}
                                    RemoteCard {ReservedFeature("中转连接", "后续可选择中转服务器")}
                                    RemoteCard {MenuRow("小窗模式",icon=Icons.Outlined.PictureInPicture) {panel=RemotePanel.Pip}}
                                    RemoteCard {MenuRow("自动隐藏工具栏") {panel=RemotePanel.HideToolbar}}
                                    RemoteCard {MenuRow("高级选项",icon=Icons.Outlined.Tune) {panel=RemotePanel.Advanced}}
                                    RemoteCard {MenuRow("操控指南",icon=Icons.Outlined.TouchApp) {guideStep=0;panel=RemotePanel.Guide}}
                                }
                                RemotePanel.Network->Column(Modifier.padding(12.dp).verticalScroll(rememberScrollState())) {
                                    RemoteCard {
                                        InfoRow(when {network.processingTarget->"帧延时（播放目标）";network.processingEstimated->"帧处理延时（估算）";else->"帧处理延时"},"${metric(network.frameProcessingMs)} ms");InfoRow("网络往返 RTT","${metric(network.rttMs)} ms");InfoRow("解码延时","${metric(network.decodeMs)} ms")
                                        InfoRow("帧率",metric(network.fps));InfoRow("带宽占用","${metric(network.mbps)} Mbps");InfoRow("丢包率","${metric(network.lossPercent)}%")
                                        InfoRow("连接时间",duration(System.currentTimeMillis()-s.startedAtMs));InfoRow("加密通道",network.transport)
                                        InfoRow("被控端 IP",network.remoteAddress);InfoRow("控制端 IP",network.localAddress)
                                        InfoRow("客户端系统","Android ${android.os.Build.VERSION.RELEASE}");InfoRow("编码",network.codec);InfoRow("解码器",network.decoder);InfoRow("画面尺寸",network.resolution)
                                        InfoRow("采集方式",c.captureMethod);InfoRow("目标帧率","${c.targetVideoFps} FPS")
                                        InfoRow("手机屏幕刷新率","${metric((context as? android.app.Activity)?.display?.refreshRate?.toDouble())} Hz")
                                    }
                                    Text("帧处理延时统计接收至解码完成；缺失时显示缓冲与解码估算，或库提供的播放目标延时并标注。播放目标包含抖动缓冲、解码预算和渲染等待；不是采集到显示的全链路延时。",style=MaterialTheme.typography.bodySmall,modifier=Modifier.padding(8.dp))
                                    ToggleRow("常驻显示网络状态",prefs.showNetwork) {c.updateRemotePreferences(prefs.copy(showNetwork=it))}
                                }
                                RemotePanel.HideToolbar->Column(Modifier.padding(12.dp)) {
                                    listOf(ToolbarHide.Mapping to "按键映射启用时隐藏",ToolbarHide.Always to "全局隐藏",ToolbarHide.Never to "不隐藏").forEach {(mode,title)->
                                        MenuRow(title,if(prefs.toolbarHide==mode) "✓" else "") {c.updateRemotePreferences(prefs.copy(toolbarHide=mode))}
                                    }
                                    Text("隐藏后点按画面右侧的“操作”展开工具栏。",style=MaterialTheme.typography.bodySmall)
                                }
                                RemotePanel.Pip->Column(Modifier.padding(12.dp)) {
                                    MenuRow("进入小窗模式") {if((context as? MainActivity)?.enterRemotePip()!=true) error="未能进入小窗，请检查系统权限" else panel=null}
                                    ToggleRow("后台运行自动进入小窗",prefs.autoPip) {c.updateRemotePreferences(prefs.copy(autoPip=it))}
                                    ToggleRow("在顶部快捷栏显示",prefs.showPipShortcut) {c.updateRemotePreferences(prefs.copy(showPipShortcut=it))}
                                }
                                RemotePanel.Guide->Column(Modifier.padding(20.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) {
                                    Text(listOf("屏幕触控与鼠标指针","鼠标与滚轮","按键映射")[guideStep],style=MaterialTheme.typography.titleLarge)
                                    Text(listOf("屏幕触控：点按或拖动对应位置。鼠标指针：单指移动，轻点左键，长按拖动；双指上下滑动滚动，捏合缩放画面。", "开启虚拟鼠标后，可按住左键拖动、点按右键和滚轮。左侧右键模式可按住，把触控操作切换为右键。", "在操作中启用按键映射，切换或编辑方案；调整按钮位置后拖动浮动按钮，点完成保存。")[guideStep])
                                    Row {if(guideStep>0) TextButton(onClick={guideStep--}) {Text("上一步")}; Button(onClick={if(guideStep<2) guideStep++ else panel=null}) {Text(if(guideStep<2) "下一步 (${guideStep+1}/3)" else "知道了")}}
                                }
                                RemotePanel.System->SystemActionsPanel(c)
                                RemotePanel.Advanced->Column(Modifier.padding(12.dp)) {
                                    ToggleRow("自适应防遮挡",prefs.avoidCutout) {c.updateRemotePreferences(prefs.copy(avoidCutout=it))}
                                    Text("鼠标速度：${metric(prefs.pointerSpeed.toDouble())}")
                                    Slider(value=prefs.pointerSpeed,onValueChange={c.updateRemotePreferences(prefs.copy(pointerSpeed=it))},valueRange=.25f..3f)
                                }
                                null->Unit
                            }
                        }
                    }
                }
            }
            if(!pip && !mappingEditing && !mappingProfiles && (toolbar || panel!=null || keyboard)) Column(Modifier.align(Alignment.CenterEnd).then(controlInsets).width(48.dp).fillMaxHeight().background(Color(0x66000000)).testTag("remote-rail"),verticalArrangement=Arrangement.SpaceEvenly,horizontalAlignment=Alignment.CenterHorizontally) {
                RailAction("操作",Icons.Outlined.Tune,panel==RemotePanel.Operation) {keyboard=false;panel=if(panel==RemotePanel.Operation) null else RemotePanel.Operation}
                RailAction("键盘",Icons.Outlined.Keyboard,keyboard,enabled) {panel=null;keyboard=!keyboard}
                RailAction("显示桌面",Icons.Outlined.DesktopWindows,false,enabled) {c.sendShortcut(InputShortcut("desktop","显示桌面",0x44,meta=true))}
                RailAction("所有窗口",Icons.Outlined.Window,false,enabled) {c.sendShortcut(InputShortcut("tasks","展示所有窗口",0x09,meta=true))}
            }
        if(!pip && keyboard) Surface(Modifier.align(Alignment.BottomCenter).fillMaxWidth(),color=MaterialTheme.colorScheme.surface) {RemoteKeyboard(c,onClose={keyboard=false},onEdit={keyboard=false;panel=null;mappingEditing=true})}
        if(!pip && mappingProfiles) MappingProfiles(c,onClose={mappingProfiles=false;panel=RemotePanel.Operation},onEdit={mappingProfiles=false;returnToProfiles=true;mappingEditing=true})
        if(!pip && mappingEditing) MappingEditorSurface(c.mappingDraft(),prefs.buttonSize,prefs.buttonOpacity,c::saveMappingDraft,onClose={mappingEditing=false;if(returnToProfiles) mappingProfiles=true else panel=RemotePanel.Operation})
        if(!pip && files) FileTransferScreen(c,onClose={files=false})
        val message=error ?: c.audioError ?: c.inputError
        if(!pip && message!=null) Surface(Modifier.align(Alignment.TopCenter).padding(8.dp),shape=RoundedCornerShape(8.dp)) {Row(verticalAlignment=Alignment.CenterVertically) {Text(message,Modifier.padding(8.dp));if(error!=null) TextButton(onClick={error=null}) {Text("关闭")}}}
    }
}

@Composable private fun RailAction(title:String,icon:ImageVector,selected:Boolean=false,enabled:Boolean=true,onClick:()->Unit) {
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(6.dp)).background(if(selected) Color(0xFF313745) else Color.Transparent).clickable(enabled=enabled,onClick=onClick).padding(vertical=6.dp),horizontalAlignment=Alignment.CenterHorizontally) {
        Icon(icon,title,tint=if(enabled) Color.White else Color.Gray,modifier=Modifier.size(20.dp));Text(title,color=if(enabled) Color.White else Color.Gray,fontSize=9.sp,lineHeight=11.sp,textAlign=androidx.compose.ui.text.style.TextAlign.Center)
    }
}
@Composable private fun QuickTile(title:String,icon:ImageVector,selected:Boolean=false,enabled:Boolean=true,onClick:()->Unit) {
    Column(Modifier.size(width=48.dp,height=44.dp).clip(RoundedCornerShape(6.dp)).background(if(selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceVariant).clickable(enabled=enabled,onClick=onClick).padding(2.dp),horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.Center) {Icon(icon,title,Modifier.size(18.dp));Text(title,style=MaterialTheme.typography.labelSmall)}
}
@Composable internal fun RemoteCard(content:@Composable ColumnScope.()->Unit) {Surface(shape=RoundedCornerShape(6.dp),color=MaterialTheme.colorScheme.surfaceVariant) {Column(Modifier.fillMaxWidth().padding(4.dp),content=content)}}
@Composable internal fun MenuRow(title:String,detail:String="",icon:ImageVector?=null,enabled:Boolean=true,onClick:()->Unit) {
    Row(Modifier.fillMaxWidth().clickable(enabled=enabled,onClick=onClick).padding(vertical=8.dp,horizontal=6.dp),verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(6.dp)) {if(icon!=null) Icon(icon,null,Modifier.size(18.dp));Text(title,Modifier.weight(1f),color=if(enabled) MaterialTheme.colorScheme.onSurface else Color.Gray);if(detail.isNotEmpty()) Text(detail,style=MaterialTheme.typography.labelSmall);Icon(Icons.Outlined.ChevronRight,null,Modifier.size(14.dp))}
}
@Composable internal fun ToggleRow(title:String,value:Boolean,enabled:Boolean=true,onChange:(Boolean)->Unit) {Row(Modifier.fillMaxWidth().padding(horizontal=6.dp,vertical=2.dp),verticalAlignment=Alignment.CenterVertically) {Text(title,Modifier.weight(1f));Checkbox(checked=value,onCheckedChange=onChange,enabled=enabled,modifier=Modifier.size(32.dp))}}
@Composable internal fun ReservedFeature(title:String,reason:String) {
    Column(Modifier.fillMaxWidth().padding(8.dp)) {
        ToggleRow(title,false,false) {}
        Text(reason,Modifier.padding(horizontal=8.dp),color=Color.LightGray,style=MaterialTheme.typography.bodySmall)
    }
}
@Composable private fun InfoRow(title:String,value:String) {Row(Modifier.fillMaxWidth().padding(8.dp)) {Text(title,Modifier.weight(1f),color=Color.LightGray,style=MaterialTheme.typography.bodySmall);Text(value,Modifier.weight(1.2f).testTag("network-value-$title"),style=MaterialTheme.typography.bodySmall)}}
private fun metric(value:Double?)=if(value==null || !value.isFinite()) "—" else String.format(java.util.Locale.ROOT,"%.1f",value)
private fun duration(ms:Long):String {val secs=(ms/1000).coerceAtLeast(0);return "%02d:%02d:%02d".format(secs/3600,secs/60%60,secs%60)}

@Composable private fun PeripheralPanel() {
    val manager=LocalContext.current.getSystemService(android.content.Context.INPUT_SERVICE) as android.hardware.input.InputManager
    val devices=manager.inputDeviceIds.toList().mapNotNull(manager::getInputDevice)
    Column(Modifier.padding(12.dp).verticalScroll(rememberScrollState())) {
        for((title,source) in listOf("键盘" to android.view.InputDevice.SOURCE_KEYBOARD,"鼠标" to android.view.InputDevice.SOURCE_MOUSE,"手柄" to android.view.InputDevice.SOURCE_GAMEPAD)) {
            Text(title,style=MaterialTheme.typography.titleMedium);Text(devices.filter { !it.isVirtual && it.supportsSource(source) }.joinToString {it.name}.ifBlank {"未连接"},Modifier.padding(vertical=12.dp))
        }
        Text("连接外接键盘或鼠标后可直接操作电脑。",style=MaterialTheme.typography.bodySmall)
        ReservedFeature("手柄转发", "安装虚拟手柄驱动后可启用")
        ReservedFeature("麦克风转发", "安装虚拟音频设备后可启用")
    }
}
