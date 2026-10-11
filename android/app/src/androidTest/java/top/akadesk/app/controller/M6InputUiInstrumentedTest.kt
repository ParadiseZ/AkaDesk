package top.akadesk.app.controller

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import top.akadesk.app.MainActivity

class M6InputUiInstrumentedTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun videoTouchToolbarAndTextFieldUseTheProductionInputChain() {
        val args=InstrumentationRegistry.getArguments()
        assumeTrue("Requires isolated M6 fixture",args.containsKey("m6HostId"))
        val coordinator=ViewModelProvider(compose.activity)[ControllerViewModel::class.java].coordinator
        val originalCodec=coordinator.preferredVideoCodec
        val originalPreferences=coordinator.remotePreferences
        val host=ControlledHostSummary("m6-ui-fixture",args.getString("m6HostId")!!,"M6 UI test","windows","fixture","test","127.0.0.1",46671,false,true,0L)
        try {
            compose.runOnIdle {
                coordinator.updatePreferredVideoCodec(ControllerVideoCodecPreset.H264)
                coordinator.updateRemotePreferences(originalPreferences.copy(mappingEnabled=false,virtualMouse=true,tabletTouch=false,toolbarHide=ToolbarHide.Never))
                coordinator.openVerificationSheet(host,args.getString("m6PairCode")!!)
                coordinator.submitVerificationConnect()
            }
            compose.waitUntil(35_000){coordinator.remoteInputReady && coordinator.hasActiveRemoteVideoTrack(coordinator.activeSession?.sessionId)}
            val x=args.getString("m6X")!!.toFloat();val y=args.getString("m6Y")!!.toFloat()
            compose.onNodeWithTag("remote-pointer",useUnmergedTree=true).performTouchInput {
                down(Offset(width*x,height*y));moveTo(Offset(width*(x+0.02f),height*(y+0.02f)),100);up()
            }
            compose.onNodeWithText("右键").performClick()
            compose.onNodeWithText("滚轮 ↑").performClick()
            compose.onNodeWithText("键盘").performClick()
            compose.onNode(hasSetTextAction()).performTextInput("UI中文🙂")
            compose.onNodeWithText("发送文字").performScrollTo().performClick()
            val instrumentation=InstrumentationRegistry.getInstrumentation()
            // Native floating IMEs can keep Espresso busy; stopping the session only
            // requires the main thread, and must not wait on IME animation idleness.
            instrumentation.runOnMainSync {assertNull(coordinator.inputError);assertTrue(coordinator.remoteInputReady)}
            // Release before moving to background; the test also exercises the same release method.
            instrumentation.runOnMainSync {coordinator.releaseRemoteInput();coordinator.endRemoteSession()}
            val deadline=System.nanoTime()+10_000_000_000L
            while(coordinator.activeSession!=null && System.nanoTime()<deadline) Thread.sleep(50)
            instrumentation.runOnMainSync {assertNull(coordinator.activeSession)}
        } finally {
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                coordinator.endRemoteSession()
                coordinator.updatePreferredVideoCodec(originalCodec)
                coordinator.updateRemotePreferences(originalPreferences)
                val fixtureIds=(args.getString("m6CleanupHostIds","")!!.split(",")+host.deviceId).filter {it.length==9 && it.all(Char::isDigit)}
                fixtureIds.forEach { id ->
                    coordinator.removeHistory(id)
                    coordinator.pairedDevices.firstOrNull{it.deviceId==id}?.let(coordinator::removePairedDevice)
                }
                coordinator.stopDiscovery()
            }
        }
    }
}
