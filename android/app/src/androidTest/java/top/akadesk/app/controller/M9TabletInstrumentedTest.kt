package top.akadesk.app.controller

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import top.akadesk.app.MainActivity

class M9TabletInstrumentedTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun twoTouchContactsMoveLiftAndCancelThroughTheNativeWindowsProbe() {
        val args=InstrumentationRegistry.getArguments()
        assumeTrue("Requires the guarded Windows input fixture",args.containsKey("m9TouchHostId"))
        val inst=InstrumentationRegistry.getInstrumentation()
        val c=ViewModelProvider(compose.activity)[ControllerViewModel::class.java].coordinator
        val originalCodec=c.preferredVideoCodec
        val host=ControlledHostSummary("m9-touch-fixture",args.getString("m9TouchHostId")!!,"Tablet test","windows","fixture","test","127.0.0.1",46671,false,true,0L)
        val x=args.getString("m9X")!!.toFloat();val y=args.getString("m9Y")!!.toFloat()
        fun frame(phase:Int,delta:Float=0f)=listOf(0,1).map {id->akadesk.v1.TouchPoint.newBuilder().setId(id).setNormalizedX(x+id*.03f+delta).setNormalizedY(y+delta).setPhase(phase).build()}
        try {
            compose.runOnIdle {c.updatePreferredVideoCodec(ControllerVideoCodecPreset.H264);c.openVerificationSheet(host,args.getString("m9TouchPairCode")!!);c.submitVerificationConnect()}
            compose.waitUntil(35_000) {c.remoteInputReady && c.hasActiveRemoteVideoTrack(c.activeSession?.sessionId)}
            inst.runOnMainSync {c.pointerInput(x,y,true);c.pointerInput(x,y,false)}
            Thread.sleep(200)
            for(points in listOf(frame(1),frame(2,.01f),frame(3,.02f))) {
                inst.runOnMainSync {c.touchInput(points)};Thread.sleep(200)
                inst.runOnMainSync {assertNull(c.inputError);assertTrue(c.remoteInputReady)}
            }
            inst.runOnMainSync {c.touchInput(frame(1))};Thread.sleep(200)
            inst.runOnMainSync {c.releaseRemoteInput()};Thread.sleep(200)
            inst.runOnMainSync {assertNull(c.inputError);assertTrue(c.remoteInputReady);c.endRemoteSession()}
            compose.waitUntil(15_000) {c.activeSession==null}
        } finally {
            inst.runOnMainSync {c.endRemoteSession();c.updatePreferredVideoCodec(originalCodec);c.removeHistory(host.deviceId);c.pairedDevices.firstOrNull {it.deviceId==host.deviceId}?.let(c::removePairedDevice);c.stopDiscovery()}
        }
    }
}
