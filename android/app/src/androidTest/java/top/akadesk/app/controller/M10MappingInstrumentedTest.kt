package top.akadesk.app.controller

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

/** Requires the M6 native probe; input cannot leave that probe. */
class M10MappingInstrumentedTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun sharedChordsToggleRepeatRelativePointerAndIdleFramesUseNativeChannels() {
        val args=InstrumentationRegistry.getArguments();assumeTrue(args.containsKey("m10HostId"))
        val inst=InstrumentationRegistry.getInstrumentation();val c=ViewModelProvider(compose.activity)[ControllerViewModel::class.java].coordinator
        val originalPrefs=c.remotePreferences;val originalCodec=c.preferredVideoCodec
        val inputPrefs=compose.activity.getSharedPreferences("akadesk-input",android.content.Context.MODE_PRIVATE);val baseline=inputPrefs.all.toMap()
        val host=ControlledHostSummary("m10-fixture",args.getString("m10HostId")!!,"Mapping test","windows","fixture","test","127.0.0.1",46671,false,true,0L)
        fun main(block:()->Unit)=inst.runOnMainSync(block)
        fun stats():org.webrtc.RTCStatsReport {
            val q=LinkedBlockingQueue<org.webrtc.RTCStatsReport>();main {c.collectRemoteMediaStats {q.offer(it)}};return q.poll(5,TimeUnit.SECONDS) ?: error("No stats")
        }
        fun counters():Pair<Long,Long> {
            val video=stats().statsMap.values.first {it.type=="inbound-rtp" && (it.members["kind"]=="video" || it.members["mediaType"]=="video")}
            return (video.members["framesDecoded"] as Number).toLong() to (video.members["bytesReceived"] as Number).toLong()
        }
        fun key(code:Int)=InputShortcut("test-$code","测试",code)
        try {
            main {c.updateRemotePreferences(originalPrefs.copy(mappingEnabled=false,toolbarHide=ToolbarHide.Never,autoPip=false));c.updatePreferredVideoCodec(ControllerVideoCodecPreset.H264);c.openVerificationSheet(host,args.getString("m10PairCode")!!);c.submitVerificationConnect()}
            compose.waitUntil(40_000) {c.remoteInputReady && c.toolsReady && c.hasActiveRemoteVideoTrack(c.activeSession?.sessionId)}
            main {c.pointerInput(.5f,.5f,true);c.pointerInput(.5f,.5f,false)};Thread.sleep(500)
            runBlocking {withContext(Dispatchers.Main) {c.toolRequest("set_video",JSONObject().put("width",960).put("height",540).put("fps",30).put("bitrate",2500))}}
            Thread.sleep(2500)
            val before=counters();val started=System.nanoTime();Thread.sleep(4500);val after=counters();val seconds=(System.nanoTime()-started)/1e9
            val idleFps=(after.first-before.first)/seconds;val idleMbps=(after.second-before.second)*8/seconds/1e6
            if(args.getString("m10VerifyIdle")=="true") {
                assertTrue("Idle fps=$idleFps",idleFps in .5..2.0);assertTrue("Idle video Mbps=$idleMbps",idleMbps<.15)
            }
            assertFalse(c.activeRemoteVideoStalled)
            // F11 only animates the isolated probe, producing a predictable changing desktop.
            main {c.sendShortcut(key(0x7a))};Thread.sleep(1500)
            val dynamicBefore=counters();Thread.sleep(2500);val dynamicAfter=counters()
            val dynamicFps=(dynamicAfter.first-dynamicBefore.first)/2.5
            assertTrue("Changed desktop fps=$dynamicFps",dynamicFps>5)
            main {c.sendShortcut(key(0x7a))};Thread.sleep(300)
            val combo=InputShortcut("four","四键",0x0d,actions=listOf(MappingAction(MappingActionKind.Key,0x11),MappingAction(MappingActionKind.Key,0x52),MappingAction(MappingActionKind.Key,0x48),MappingAction(MappingActionKind.Mouse,2)))
            val overlapping=InputShortcut("overlap","共享 Ctrl",0x4a,ctrl=true)
            main {c.mappingPress(combo,true);c.mappingPress(overlapping,true);c.mappingPress(combo,false);assertEquals(setOf("overlap"),c.activeMappingIds);c.mappingPress(overlapping,false)}
            val toggle=key(0x76).copy(behavior=MappingBehavior.Toggle)
            main {c.mappingPress(toggle,true);c.mappingPress(toggle,false);assertTrue(toggle.id in c.activeMappingIds)};Thread.sleep(150)
            main {c.mappingPress(toggle,true);assertTrue(c.activeMappingIds.isEmpty())}
            val repeat=key(0x77).copy(behavior=MappingBehavior.Repeat)
            main {c.mappingPress(repeat,true)};Thread.sleep(350);main {c.mappingPress(repeat,false)}
            main {
                assertTrue(c.saveMappingDraft(c.mappingDraft().copy(view=MappingViewControl(true,ViewTrigger.Right))))
                c.beginViewControl();c.moveViewControl(.005f,.005f);c.moveViewControl(.005f,.005f);c.endViewControl()
                c.mappingPress(key(0x78),true);c.releaseRemoteInput();assertTrue(c.activeMappingIds.isEmpty())
            }
            Thread.sleep(500);assertNull(c.inputError)
            inst.sendStatus(0,android.os.Bundle().apply {putString("mappingAcceptance","baseline fps=$idleFps, video Mbps=$idleMbps, changed fps=$dynamicFps, idle assertion enabled=${args.getString("m10VerifyIdle")=="true"}; four-action chord/shared Ctrl/toggle/repeat/relative pointer/release")})
        } finally {
            main {c.endRemoteSession();c.updateRemotePreferences(originalPrefs);c.updatePreferredVideoCodec(originalCodec);c.removeHistory(host.deviceId);c.pairedDevices.firstOrNull {it.deviceId==host.deviceId}?.let(c::removePairedDevice);c.stopDiscovery()}
            val editor=inputPrefs.edit().clear();baseline.forEach {(k,v)->when(v) {is String->editor.putString(k,v);is Float->editor.putFloat(k,v);is Int->editor.putInt(k,v);is Boolean->editor.putBoolean(k,v);is Long->editor.putLong(k,v)}};editor.commit()
        }
    }
}
