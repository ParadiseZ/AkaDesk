package top.akadesk.app.controller

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import top.akadesk.app.MainActivity
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

class M8RecoveryInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    @Test fun actualControlReconnectPreservesVideoAndSessionThenStopsCleanly() {
        val args=InstrumentationRegistry.getArguments()
        assumeTrue("Requires isolated M8 fixture",args.containsKey("m8HostId"))
        val coordinator=ViewModelProvider(compose.activity)[ControllerViewModel::class.java].coordinator
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val originalCodec=coordinator.preferredVideoCodec
        val host=ControlledHostSummary("m8-fixture",args.getString("m8HostId")!!,"M8 recovery test","windows","fixture","test","127.0.0.1",46671,false,true,0L)
        fun field(name:String):Any? = coordinator.javaClass.getDeclaredField(name).apply {isAccessible=true}.get(coordinator)
        fun frames():Long {
            val result=LinkedBlockingQueue<Long>()
            instrumentation.runOnMainSync {coordinator.collectRemoteMediaStats {report ->
                result.offer(report.statsMap.values.filter {it.type=="inbound-rtp" && (it.members["kind"]=="video" || it.members["mediaType"]=="video")}
                    .sumOf {(it.members["framesDecoded"] as? Number)?.toLong() ?: 0L})
            }}
            return result.poll(5,TimeUnit.SECONDS) ?: error("Missing decoded video statistics")
        }
        try {
            compose.runOnIdle {
                coordinator.updatePreferredVideoCodec(ControllerVideoCodecPreset.H264)
                coordinator.openVerificationSheet(host,args.getString("m8PairCode")!!)
                coordinator.submitVerificationConnect()
            }
            compose.waitUntil(35_000) {coordinator.remoteInputReady && coordinator.hasActiveRemoteVideoTrack(coordinator.activeSession?.sessionId)}
            val sessionId=coordinator.activeSession!!.sessionId!!
            val before=frames()
            lateinit var originalMedia:Any
            lateinit var originalControl:Any
            // Fault injection cancels the real WebSocket through its ordinary failure
            // path. No extra production API is introduced for this test.
            instrumentation.runOnMainSync {
                originalMedia=field("webRtcSession")!!;originalControl=field("controlSession")!!
                originalControl.javaClass.getDeclaredMethod("notifyFailure",String::class.java).apply {isAccessible=true}.invoke(originalControl,"M8 control disconnect injection")
            }
            compose.waitUntil(25_000) {field("controlSession") != null && field("controlSession") !== originalControl && field("pendingControlReconnectHost")==null}
            instrumentation.runOnMainSync {
                assertEquals(sessionId,coordinator.activeSession!!.sessionId)
                assertSame(originalMedia,field("webRtcSession"))
                coordinator.refreshDisplaySettings()
            }
            compose.waitUntil(15_000) {!coordinator.displaySettingsPending}
            assertNull(coordinator.displaySettingsError);assertNotNull(coordinator.displaySettings)
            var after=frames();val deadline=System.nanoTime()+8_000_000_000L
            while(after<before+3 && System.nanoTime()<deadline) {Thread.sleep(150);after=frames()}
            assertTrue("No new video after control recovery: $before -> $after",after>=before+3)
            instrumentation.runOnMainSync {coordinator.endRemoteSession()}
            compose.waitUntil(15_000) {coordinator.activeSession==null}
            instrumentation.sendStatus(0,android.os.Bundle().apply {putString("m8RecoveredVideoFrames","$before->$after");putString("m8PreservedSession","true")})
        } finally {
            instrumentation.runOnMainSync {
                coordinator.endRemoteSession();coordinator.updatePreferredVideoCodec(originalCodec)
                coordinator.removeHistory(host.deviceId)
                coordinator.pairedDevices.firstOrNull {it.deviceId==host.deviceId}?.let(coordinator::removePairedDevice)
                coordinator.stopDiscovery()
            }
        }
    }
}
