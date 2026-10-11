package top.akadesk.app.ui.home

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import org.json.JSONObject
import top.akadesk.app.controller.ControllerCoordinator

@Composable
fun FileTransferScreen(c:ControllerCoordinator,onClose:()->Unit) {
    val scope=rememberCoroutineScope()
    var browsing by rememberSaveable {mutableStateOf(false)}
    var path by rememberSaveable {mutableStateOf("")}
    var root by remember {mutableStateOf("")}
    var entries by remember {mutableStateOf<List<JSONObject>>(emptyList())}
    var error by remember {mutableStateOf<String?>(null)}
    var pendingPath by rememberSaveable {mutableStateOf("")}
    var pendingName by rememberSaveable {mutableStateOf("")}
    var busy by remember {mutableStateOf(false)}
    val pick=rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) {uris->uris.forEach(c.fileTransfers::upload)}
    val save=rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) {uri->if(uri!=null && pendingPath.isNotEmpty()) c.fileTransfers.download(pendingPath,pendingName,uri);pendingPath=""}
    suspend fun refresh() {
        busy=true;error=null
        try {val response=c.toolRequest("list",JSONObject().put("path",path));root=response.optString("root");val array=response.getJSONArray("entries");entries=(0 until array.length()).map(array::getJSONObject).sortedWith(compareBy({!it.getBoolean("directory")},{it.getString("name")}))}
        catch(e:Exception) {error=e.message ?: "无法读取电脑文件"}
        finally {busy=false}
    }
    LaunchedEffect(browsing,path,c.toolsReady) {if(c.toolsReady) {if(browsing) refresh() else runCatching {root=c.toolRequest("status").optString("root")}}}
    Surface(Modifier.fillMaxSize(),color=MaterialTheme.colorScheme.surface) {
        Column(Modifier.padding(16.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(12.dp)) {
            Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) {
                TextButton(onClick={if(browsing) browsing=false else onClose()}) {Text(if(browsing) "返回传输" else "关闭")}
                Text(c.activeSession?.host?.displayName ?: "文件传输",style=MaterialTheme.typography.titleLarge)
                TextButton(onClick={c.fileTransfers.clearHistory()}) {Text("清空记录")}
            }
            if(browsing) {
                Row(horizontalArrangement=Arrangement.spacedBy(12.dp)) {
                    TextButton(onClick={path=path.substringBeforeLast('/',"")},enabled=path.isNotEmpty()) {Text("上一级")}
                    Text(path.ifEmpty {"共享文件夹"},Modifier.weight(1f))
                    TextButton(onClick={scope.launch {refresh()}},enabled=!busy && c.toolsReady) {Text("刷新")}
                }
                if(busy) LinearProgressIndicator(Modifier.fillMaxWidth())
                entries.forEach {entry->
                    val name=entry.getString("name");val directory=entry.getBoolean("directory")
                    Row(Modifier.fillMaxWidth().clickable(enabled=!busy) {val target=if(path.isEmpty()) name else "$path/$name";if(directory) path=target else {pendingPath=target;pendingName=name;save.launch(name)}}.padding(12.dp),horizontalArrangement=Arrangement.SpaceBetween) {
                        Text(if(directory) "📁 $name" else name);Text(if(directory) "打开" else "${entry.getLong("size")/1024} KB · 下载")
                    }
                }
                if(entries.isEmpty() && !busy && error==null) Text("电脑共享文件夹中还没有文件")
            } else {
                Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(12.dp)) {
                    OutlinedButton(onClick={pick.launch(arrayOf("image/*","video/*"))},enabled=c.toolsReady,modifier=Modifier.weight(1f)) {Text("发送照片 & 视频")}
                    OutlinedButton(onClick={pick.launch(arrayOf("*/*"))},enabled=c.toolsReady,modifier=Modifier.weight(1f)) {Text("发送文件")}
                    OutlinedButton(onClick={browsing=true},enabled=c.toolsReady,modifier=Modifier.weight(1f)) {Text("从电脑下载到手机")}
                }
                for((title,filter) in listOf("传输中" to 0,"已接收" to 1,"已发送" to 2,"失败 / 已取消" to 3)) {
                    val records=c.fileTransfers.records.filter {when(filter){0->it.status=="传输中";1->it.status=="已接收";2->it.status=="已发送";else->it.status in listOf("传输失败","已取消")}}
                    var expanded by rememberSaveable(title) {mutableStateOf(true)}
                    TextButton(onClick={expanded=!expanded}) {Text("$title (${records.size}) ${if(expanded) "⌃" else "⌄"}")}
                    if(expanded) records.asReversed().forEach {record->
                        Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(12.dp)) {
                            Column(Modifier.weight(1f)) {
                                Text(record.name);Text("${record.completed/1024} / ${record.size/1024} KB · ${record.status}",style=MaterialTheme.typography.bodySmall)
                                if(record.status=="传输中") LinearProgressIndicator(progress={if(record.size>0) (record.completed.toFloat()/record.size).coerceIn(0f,1f) else 0f},modifier=Modifier.fillMaxWidth())
                                record.error?.let {Text(it,color=MaterialTheme.colorScheme.error,style=MaterialTheme.typography.bodySmall)}
                            }
                            if(record.status=="传输中") TextButton(onClick={c.fileTransfers.cancel(record.id)}) {Text("取消")}
                        }
                    }
                }
            }
            if(!c.toolsReady) Text("正在等待电脑文件传输通道…")
            error?.let {Text(it,color=MaterialTheme.colorScheme.error)}
            if(root.isNotEmpty()) Text("电脑文件保存至：$root\n下载列表显示此共享文件夹中的文件。",style=MaterialTheme.typography.bodySmall)
        }
    }
}
