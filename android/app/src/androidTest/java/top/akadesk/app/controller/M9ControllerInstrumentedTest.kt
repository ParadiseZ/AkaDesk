package top.akadesk.app.controller

import android.content.ContentValues
import android.provider.MediaStore
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import top.akadesk.app.MainActivity
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

/** Real Windows/Android WebRTC, production Compose controls and scoped-storage transfers. */
class M9ControllerInstrumentedTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun controllerPanelsQualityViewOnlyClipboardAndFilesUseLiveChannels() {
        val args=InstrumentationRegistry.getArguments()
        assumeTrue("Requires isolated controller fixture",args.containsKey("m9HostId"))
        val inst=InstrumentationRegistry.getInstrumentation()
        val c=ViewModelProvider(compose.activity)[ControllerViewModel::class.java].coordinator
        val context=compose.activity
        val audioManager=context.getSystemService(android.content.Context.AUDIO_SERVICE) as android.media.AudioManager
        val originalMode=audioManager.mode;val originalSpeaker=audioManager.isSpeakerphoneOn
        val originalMediaVolume=audioManager.getStreamVolume(android.media.AudioManager.STREAM_MUSIC)
        val originalCallVolume=audioManager.getStreamVolume(android.media.AudioManager.STREAM_VOICE_CALL)
        val otherMedia=android.media.AudioTrack.Builder()
            .setAudioAttributes(android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_MEDIA).setContentType(android.media.AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setAudioFormat(android.media.AudioFormat.Builder().setSampleRate(48000).setChannelMask(android.media.AudioFormat.CHANNEL_OUT_MONO).setEncoding(android.media.AudioFormat.ENCODING_PCM_16BIT).build())
            .setTransferMode(android.media.AudioTrack.MODE_STATIC).setBufferSizeInBytes(9600).build()
        otherMedia.write(ByteArray(9600),0,9600);otherMedia.setLoopPoints(0,4800,-1);otherMedia.play()
        val resolver=context.contentResolver
        val originalCodec=c.preferredVideoCodec
        val originalPreferences=c.remotePreferences
        val mappingPreferences=context.getSharedPreferences("akadesk-input",android.content.Context.MODE_PRIVATE)
        val originalMappings=mappingPreferences.all.toMap()
        val transferPreferences=context.getSharedPreferences("akadesk-transfers",android.content.Context.MODE_PRIVATE)
        val originalTransferHistory=transferPreferences.getString("history","[]")
        val host=ControlledHostSummary("m9-fixture",args.getString("m9HostId")!!,"Controller test","windows","fixture","test","127.0.0.1",46671,false,true,0L)
        val filename="controller-${System.currentTimeMillis()}.bin"
        val bytes=ByteArray(49_337){(it*31).toByte()}
        var source:android.net.Uri?=null
        var destination:android.net.Uri?=null
        var clipboardOriginal:String?=null
        fun tool(op:String,args:JSONObject=JSONObject()):JSONObject=runBlocking {withContext(Dispatchers.Main) {c.toolRequest(op,args)}}
        fun frames():Long {
            val queue=LinkedBlockingQueue<Long>()
            inst.runOnMainSync {c.collectRemoteMediaStats {report->queue.offer(report.statsMap.values.filter {it.type=="inbound-rtp" && (it.members["kind"]=="video" || it.members["mediaType"]=="video")}.sumOf {(it.members["framesDecoded"] as? Number)?.toLong() ?: 0L})}}
            return queue.poll(5,TimeUnit.SECONDS) ?: error("No media statistics")
        }
        fun waitFor(condition:()->Boolean,timeout:Long=20_000) {
            val end=System.nanoTime()+timeout*1_000_000
            while(!condition() && System.nanoTime()<end) Thread.sleep(100)
            assertTrue("Condition timed out",condition())
        }
        fun snapshot(name:String) {
            compose.waitForIdle()
            Thread.sleep(250) // allow the surface compositor to present the changed overlay
            val bitmap=inst.uiAutomation.takeScreenshot() ?: return
            java.io.File(context.getExternalFilesDir(null),"controller-$name.png").outputStream().use {bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)}
            bitmap.recycle()
        }
        try {
            compose.runOnIdle {
                c.updateRemotePreferences(originalPreferences.copy(mappingEnabled=false,toolbarHide=ToolbarHide.Never,autoPip=false))
                c.updatePreferredVideoCodec(ControllerVideoCodecPreset.H264)
                c.openVerificationSheet(host,args.getString("m9PairCode")!!);c.submitVerificationConnect()
            }
            compose.waitUntil(40_000) {c.remoteInputReady && c.toolsReady && c.hasActiveRemoteVideoTrack(c.activeSession?.sessionId)}
            compose.waitUntil(15_000) {c.hasActiveAudioPlayout()}
            assertEquals(originalMode,audioManager.mode);assertEquals(originalSpeaker,audioManager.isSpeakerphoneOn)
            assertEquals(originalMediaVolume,audioManager.getStreamVolume(android.media.AudioManager.STREAM_MUSIC))
            assertEquals(originalCallVolume,audioManager.getStreamVolume(android.media.AudioManager.STREAM_VOICE_CALL))
            assertEquals(android.media.AudioTrack.PLAYSTATE_PLAYING,otherMedia.playState);assertTrue(otherMedia.playbackHeadPosition>4800)
            val videoBounds=compose.onNodeWithTag("remote-pointer").fetchSemanticsNode().boundsInRoot
            compose.onNodeWithText("操作").performClick()
            assertEquals(videoBounds,compose.onNodeWithTag("remote-pointer").fetchSemanticsNode().boundsInRoot)
            snapshot("operation")
            compose.onNodeWithText("仅观看").performScrollTo().performClick()
            compose.waitUntil(20_000) {c.viewOnly && !c.viewOnlyPending}
            compose.runOnIdle {assertFalse(c.sendText("view-only must reject input"))}
            compose.onAllNodesWithText("仅观看").filter(hasClickAction()).onFirst().performScrollTo().performClick()
            compose.waitUntil(20_000) {!c.viewOnly && !c.viewOnlyPending}
            compose.onNodeWithText("画质与帧率",substring=false).performScrollTo().performClick()
            val before=frames()
            compose.onNodeWithText("清晰").performClick()
            compose.onNodeWithText("应用画质").performScrollTo().performClick()
            waitFor({tool("status").let {!it.optBoolean("videoPending") && it.getJSONObject("video").getInt("bitrate")==2000}})
            waitFor({frames()>=before+5},15_000)
            // Change encoder dimensions live and confirm decoding survives SPS changes.
            tool("set_video",JSONObject().put("width",1280).put("height",720).put("fps",60).put("bitrate",8000))
            waitFor({!tool("status").optBoolean("videoPending")})
            val after=frames();waitFor({frames()>=after+5},15_000)
            assertEquals(1280,tool("status").getJSONObject("video").getInt("width"))
            snapshot("video-quality")
            compose.onNodeWithContentDescription("返回").performClick()
            compose.onNodeWithText("更多").performScrollTo().performClick()
            compose.onNodeWithText("网络状态",substring=false).performScrollTo().performClick()
            compose.onNodeWithText("带宽占用").assertExists()
            compose.mainClock.advanceTimeBy(2500)
            Thread.sleep(200)
            compose.mainClock.advanceTimeBy(2500)
            Thread.sleep(200)
            compose.onNodeWithTag("network-value-编码",useUnmergedTree=true).assertTextEquals("video/H264")
            compose.onNodeWithTag("network-value-采集方式",useUnmergedTree=true).assertTextEquals("GDI / BitBlt")
            compose.runOnIdle {assertEquals(60,c.targetVideoFps)}
            compose.onNodeWithTag("network-value-帧率",useUnmergedTree=true).assert(hasText("—",substring=true).not())
            compose.onNodeWithTag("network-value-带宽占用",useUnmergedTree=true).assert(hasText("—",substring=true).not())
            snapshot("network")
            compose.onNodeWithContentDescription("关闭面板").performClick()
            compose.onNodeWithText("键盘",substring=false).performClick()
            compose.onNodeWithText("电脑键盘").performClick()
            compose.onNodeWithText("F12").assertExists()
            snapshot("keyboard")
            compose.onNodeWithText("Esc").performClick()
            compose.onNodeWithText("关闭",substring=false).performClick()
            compose.runOnIdle {c.updateRemotePreferences(c.remotePreferences.copy(mappingEnabled=true))}
            compose.onNodeWithText("操作",substring=false).performClick()
            compose.onNodeWithText("编辑当前方案").performScrollTo().performClick()
            compose.onNodeWithTag("mapping-editor").assertExists()
            compose.onNodeWithContentDescription("＋ 组合键").performScrollTo().performClick()
            for(title in listOf("Ctrl","R","H")) compose.onNodeWithText(title,substring=false).performScrollTo().performClick()
            compose.onNodeWithText("鼠标",substring=false).performClick();compose.onNodeWithText("右键",substring=false).performClick()
            compose.onNodeWithText("确认组合键").performClick()
            compose.onNodeWithContentDescription("编辑按键名称").performClick()
            compose.onNodeWithTag("mapping-key-name").performTextReplacement("组合验收")
            compose.onNodeWithText("确定").performClick()
            compose.onNodeWithText("锁定",substring=false).performScrollTo().performClick()
            snapshot("mapping-editor")
            compose.onNodeWithText("保存方案").performClick()
            compose.runOnIdle {val value=c.inputShortcuts.single {it.label=="组合验收"};assertEquals(4,value.actions.size);assertEquals(MappingBehavior.Toggle,value.behavior)}
            tool("clipboard_enable",JSONObject().put("enabled",true))
            clipboardOriginal=tool("clipboard_get").getString("text")
            java.io.File(context.filesDir,"controller-clipboard-recovery.txt").writeText(clipboardOriginal!!)
            tool("clipboard_set",JSONObject().put("text","AkaDesk中文🙂 $filename"))
            assertEquals("AkaDesk中文🙂 $filename",tool("clipboard_get").getString("text"))
            tool("clipboard_set",JSONObject().put("text",clipboardOriginal))
            tool("clipboard_enable",JSONObject().put("enabled",false))
            java.io.File(context.filesDir,"controller-clipboard-recovery.txt").delete()
            source=resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI,ContentValues().apply {put(MediaStore.MediaColumns.DISPLAY_NAME,filename);put(MediaStore.MediaColumns.MIME_TYPE,"application/octet-stream")})!!
            resolver.openOutputStream(source!!)?.use {it.write(bytes)} ?: error("No upload output")
            inst.runOnMainSync {c.fileTransfers.upload(source!!)}
            waitFor({c.fileTransfers.records.any {it.name==filename && it.status!="传输中"}})
            assertEquals("已发送",c.fileTransfers.records.last {it.name==filename}.status)
            assertTrue(tool("list").getJSONArray("entries").toString().contains(filename))
            destination=resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI,ContentValues().apply {put(MediaStore.MediaColumns.DISPLAY_NAME,"received-$filename");put(MediaStore.MediaColumns.MIME_TYPE,"application/octet-stream")})!!
            inst.runOnMainSync {c.fileTransfers.download(filename,filename,destination!!)}
            waitFor({c.fileTransfers.records.any {it.name==filename && it.receiving && it.status!="传输中"}})
            assertEquals("已接收",c.fileTransfers.records.last {it.name==filename && it.receiving}.status)
            assertArrayEquals(bytes,resolver.openInputStream(destination!!)?.use {it.readBytes()})
            inst.sendStatus(0,android.os.Bundle().apply {putString("controllerAcceptance","live H264 dimension/fps change, view-only, PC keyboard, RTC stats, Unicode clipboard, ${bytes.size}-byte bidirectional scoped-storage transfer")})
        } finally {
            otherMedia.stop();otherMedia.release()
            if(c.toolsReady && clipboardOriginal!=null) runCatching {tool("clipboard_set",JSONObject().put("text",clipboardOriginal));tool("clipboard_enable",JSONObject().put("enabled",false))}
            inst.runOnMainSync {
                c.endRemoteSession();c.updatePreferredVideoCodec(originalCodec);c.updateRemotePreferences(originalPreferences)
                c.removeHistory(host.deviceId);c.pairedDevices.firstOrNull {it.deviceId==host.deviceId}?.let(c::removePairedDevice);c.stopDiscovery()
            }
            source?.let {resolver.delete(it,null,null)};destination?.let {resolver.delete(it,null,null)}
            transferPreferences.edit().putString("history",originalTransferHistory).commit()
            val mappingRestore=mappingPreferences.edit().clear()
            originalMappings.forEach {(key,value)->when(value) {is String->mappingRestore.putString(key,value);is Float->mappingRestore.putFloat(key,value);is Int->mappingRestore.putInt(key,value);is Boolean->mappingRestore.putBoolean(key,value);is Long->mappingRestore.putLong(key,value)}}
            mappingRestore.commit();inst.runOnMainSync {c.selectInputProfile(InputProfileStore(context).selectedProfile())}
        }
    }
}
