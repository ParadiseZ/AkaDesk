package top.akadesk.app.ui.home

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONObject
import top.akadesk.app.controller.ControllerCoordinator

@Composable
fun RemoteSecurityPanel(c:ControllerCoordinator) {
    var busy by remember {mutableStateOf(false)}
    var error by remember {mutableStateOf<String?>(null)}
    val scope=rememberCoroutineScope()
    LaunchedEffect(c.toolsReady) {
        if(c.toolsReady) runCatching {val status=c.toolRequest("status");c.lockAfterSession=status.optBoolean("lockAfter");c.clipboardSync=status.optBoolean("clipboard")}
    }
    Column(Modifier.padding(12.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(12.dp)) {
        RemoteCard {ReservedFeature("自动解锁", "安装电脑后台服务后可配置")}
        RemoteCard {ReservedFeature("防窥模式", "安装电脑后台服务及显示驱动后可启用")}
        RemoteCard {
            ToggleRow("远程结束后，被控端锁屏",c.lockAfterSession,c.toolsReady && !busy) {value->
                scope.launch {busy=true;try {c.toolRequest("lock_after",JSONObject().put("enabled",value));c.lockAfterSession=value;error=null}catch(e:Exception){error=e.message}finally {busy=false}}
            }
        }
        RemoteCard {
            ToggleRow("被控端静音运行",c.computerAudioMuted,c.computerAudioAvailable && !c.audioMutePending) {c.toggleComputerAudioMute()}
            Text("断开后恢复电脑连接前的声音设置。",Modifier.padding(8.dp),style=MaterialTheme.typography.bodySmall)
        }
        RemoteCard {
            ToggleRow("同步剪贴板",c.clipboardSync,c.toolsReady && !busy) {value->
                scope.launch {busy=true;try {c.toolRequest("clipboard_enable",JSONObject().put("enabled",value));c.clipboardSync=value;error=null}catch(e:Exception){error=e.message}finally {busy=false}}
            }
            Text("开启后，在前台同步手机和电脑复制的文字。",Modifier.padding(8.dp),style=MaterialTheme.typography.bodySmall)
        }
        error?.let {Text(it,color=MaterialTheme.colorScheme.error)}
    }
}

@Composable
internal fun ClipboardSync(c:ControllerCoordinator) {
    val context=LocalContext.current
    val clipboard=context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
    val scope=rememberCoroutineScope()
    var last by remember(c.activeSession?.sessionId) {mutableStateOf<String?>(null)}
    var revision by remember(c.activeSession?.sessionId) {mutableLongStateOf(0L)}
    val active=c.clipboardSync && c.toolsReady && c.inputForeground && !c.pictureInPicture
    DisposableEffect(active) {
        val listener=ClipboardManager.OnPrimaryClipChangedListener {
            if(active) {
                val text=clipboard.primaryClip?.takeIf {it.itemCount>0}?.getItemAt(0)?.text?.toString()
                if(text!=null && text!=last && text.toByteArray().size<=65_536) {last=text;revision++;scope.launch {runCatching {c.toolRequest("clipboard_set",JSONObject().put("text",text))}}}
            }
        }
        if(active) clipboard.addPrimaryClipChangedListener(listener)
        onDispose {clipboard.removePrimaryClipChangedListener(listener)}
    }
    LaunchedEffect(active) {
        if(active) while(true) {
            val expectedRevision=revision
            runCatching {c.toolRequest("clipboard_get").optString("text")}.onSuccess {text->
                if(expectedRevision==revision) {
                    if(last==null) {last=text} else if(last!=text) {last=text;clipboard.setPrimaryClip(ClipData.newPlainText("AkaDesk",text))}
                }
            }
            delay(2000)
        }
    }
}

@Composable
fun SystemActionsPanel(c:ControllerCoordinator) {
    var action by remember {mutableStateOf<String?>(null)}
    var busy by remember {mutableStateOf(false)}
    var error by remember {mutableStateOf<String?>(null)}
    val scope=rememberCoroutineScope()
    val names=mapOf("lock" to "锁定桌面","restart" to "重启电脑","shutdown" to "关机","task_manager" to "任务管理器")
    fun execute(value:String) {scope.launch {busy=true;error=null;try {c.releaseRemoteInput();c.toolRequest("system",JSONObject().put("action",value).put("confirmed",true))}catch(e:Exception){error=e.message}finally {busy=false}}}
    Column(Modifier.padding(12.dp).verticalScroll(rememberScrollState())) {
        names.forEach {(key,title)->MenuRow(title,enabled=c.toolsReady && !busy && !c.viewOnly) {if(key=="task_manager") execute(key) else action=key}}
        error?.let {Text(it,color=MaterialTheme.colorScheme.error)}
        if(busy) LinearProgressIndicator(Modifier.fillMaxWidth())
    }
    action?.let {value->AlertDialog(onDismissRequest={action=null},title={Text(names[value] ?: "系统操作")},text={Text(if(value=="lock") "锁定后可能需要在电脑上解锁，确定继续？" else "电脑将在 5 秒后${names[value]}，当前连接会中断。确定继续？")},
        confirmButton={TextButton(onClick={action=null;execute(value)}) {Text("确定")}},dismissButton={TextButton(onClick={action=null}) {Text("取消")}})}
}
