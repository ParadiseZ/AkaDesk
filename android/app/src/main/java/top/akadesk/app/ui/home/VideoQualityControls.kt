package top.akadesk.app.ui.home

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONObject
import top.akadesk.app.controller.ControllerCoordinator

@Composable
fun VideoQualityControls(c:ControllerCoordinator) {
    var width by remember {mutableIntStateOf(960)}
    var height by remember {mutableIntStateOf(540)}
    var fps by remember {mutableIntStateOf(30)}
    var bitrate by remember {mutableIntStateOf(2500)}
    var busy by remember {mutableStateOf(false)}
    var error by remember {mutableStateOf<String?>(null)}
    var preset by remember {mutableStateOf("自动")}
    val scope=rememberCoroutineScope()
    LaunchedEffect(c.toolsReady) {if(c.toolsReady) runCatching {val status=c.toolRequest("status");c.applyConfirmedVideoStatus(status);status.optJSONObject("video")?.let {width=it.getInt("width");height=it.getInt("height");fps=it.getInt("fps");bitrate=it.getInt("bitrate")}}}
    fun apply() {
        scope.launch {
            busy=true;error=null
            try {
                c.releaseRemoteInput()
                c.toolRequest("set_video",JSONObject().put("width",width).put("height",height).put("fps",fps).put("bitrate",bitrate))
                var confirmed=false
                repeat(30) {
                    if(!confirmed) {delay(500);val status=c.toolRequest("status");if(!status.optBoolean("videoPending")) {require(status.optString("videoError").isEmpty()) {"电脑未能应用画质设置"};c.applyConfirmedVideoStatus(status);confirmed=true}}
                }
                require(confirmed) {"未能确认画质设置，请重新打开画质与帧率页面查看"}
            } catch(e:Exception) {error=e.message} finally {busy=false}
        }
    }
    Column(Modifier.padding(12.dp),verticalArrangement=Arrangement.spacedBy(6.dp)) {
        Text("视频画质",style=MaterialTheme.typography.titleMedium)
        Row(Modifier.horizontalScroll(rememberScrollState()),horizontalArrangement=Arrangement.spacedBy(4.dp)) {
            listOf("自动","清晰","高清","原画","自定义").forEach {title->FilterChip(selected=preset==title,enabled=!busy,onClick={preset=title;when(title){"自动"->{width=960;height=540;bitrate=2500};"清晰"->{width=960;height=540;bitrate=2000};"高清"->{width=1280;height=720;bitrate=8000};"原画"->{val mode=c.displaySettings?.currentMode;width=(mode?.width ?: 1920).coerceIn(640,1920)/2*2;height=(mode?.height ?: 1080).coerceIn(360,1200)/2*2;bitrate=20000}}},label={Text(title)})}
        }
        Row(Modifier.horizontalScroll(rememberScrollState()),horizontalArrangement=Arrangement.spacedBy(4.dp)) {
            listOf(30,60,90,120).forEach {value->FilterChip(selected=fps==value,onClick={fps=value},enabled=!busy,label={Text("$value 帧")})}
        }
        if(preset=="自定义") {
            Text("码率：$bitrate kbps")
            Slider(value=bitrate.toFloat(),onValueChange={bitrate=it.toInt()/100*100},valueRange=500f..50000f,enabled=!busy)
            Text("画面宽度：$width")
            Slider(value=width.toFloat(),onValueChange={width=it.toInt()/2*2;height=(width*9/16).coerceIn(360,1200)/2*2},valueRange=640f..1920f,enabled=!busy)
        }
        Text("${width} × $height · $fps 帧 · ${bitrate/1000f} Mbps",style=MaterialTheme.typography.bodySmall)
        Text("电脑端目标与手机渲染上限：${c.targetVideoFps} FPS。静态画面降到约 1 FPS；实际帧率见网络状态。",style=MaterialTheme.typography.bodySmall)
        Button(onClick=::apply,enabled=c.toolsReady && !busy) {Text(if(busy) "正在应用…" else "应用画质")}
        error?.let {Text(it,color=MaterialTheme.colorScheme.error)}
    }
}
