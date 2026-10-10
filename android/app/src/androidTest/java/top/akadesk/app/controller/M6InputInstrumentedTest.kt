package top.akadesk.app.controller

import akadesk.v1.*
import android.os.Bundle
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.*
import java.util.concurrent.atomic.AtomicReference

@RunWith(AndroidJUnit4::class)
class M6InputInstrumentedTest {
    @Test fun orderedInputReachesWindowsAndHeldInputIsReleasedOnStop() {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val args=InstrumentationRegistry.getArguments()
        assumeTrue("Requires the isolated Windows M6 native probe fixture",args.containsKey("m6HostId"))
        val inputReady=CountDownLatch(1);val frame=CountDownLatch(1);val stopped=CountDownLatch(1)
        val error=AtomicReference<String?>(null);val id=AtomicReference<String?>(null)
        val media=AtomicReference<AndroidWebRtcSession?>(null)
        val results=LinkedBlockingQueue<InputResult>()
        val host=ControlledHostSummary("m6-fixture",args.getString("m6HostId")!!,"M6 test","windows","fixture","test","127.0.0.1",46671,false,true,0L)
        lateinit var control:HostControlSession
        fun fail(message:String){error.compareAndSet(null,message);inputReady.countDown();frame.countDown()}
        control=HostControlSession(host,"200000006",null,object:HostControlSession.Listener {
            override fun onAuthenticated(host:ControlledHostSummary)=Unit
            override fun onPairingRequired(host:ControlledHostSummary,message:String,errorCode:String){control.requestPairing("M6 input test")}
            override fun onPairingChallenge(host:ControlledHostSummary,challengeId:String,challengeType:PairingChallengeType){control.confirmPairing(args.getString("m6PairCode")!!)}
            override fun onPairingSucceeded(host:ControlledHostSummary,pairToken:String)=Unit
            override fun onDeviceSnapshotLoaded(host:ControlledHostSummary){control.requestRemoteSession(VideoCodec.VIDEO_CODEC_H264)}
            override fun onSessionAccepted(host:ControlledHostSummary,sessionId:String,stateLabel:String){id.set(sessionId)}
            override fun onSessionChanged(host:ControlledHostSummary,sessionId:String,stateLabel:String,summary:String)=Unit
            override fun onWebRtcOffer(host:ControlledHostSummary,offer:WebRtcOffer){instrumentation.runOnMainSync {
                val rtc=AndroidWebRtcSession(instrumentation.targetContext,offer.sessionId,object:AndroidWebRtcSession.Callbacks {
                    override fun onStateChanged(sessionId:String,detail:String)=Unit
                    override fun onLocalAnswer(sessionId:String,sdp:String){control.sendWebRtcAnswer(sessionId,sdp)}
                    override fun onLocalIceCandidate(sessionId:String,candidate:String,sdpMid:String,sdpMlineIndex:Int){control.sendIceCandidate(sessionId,candidate,sdpMid,sdpMlineIndex)}
                    override fun onRemoteVideoTrack(sessionId:String)=Unit
                    override fun onRemoteAudioTrack(sessionId:String)=Unit
                    override fun onRemoteVideoFirstFrame(sessionId:String,width:Int,height:Int){frame.countDown()}
                    override fun onRemoteVideoStalled(sessionId:String,stalledForMs:Long,frameCount:Int)=Unit
                    override fun onRemoteVideoFlowResumed(sessionId:String,width:Int,height:Int,frameCount:Int)=Unit
                    override fun onFailure(sessionId:String,message:String)=fail(message)
                    override fun onInputStateChanged(sessionId:String,ready:Boolean){if(ready)inputReady.countDown()}
                    override fun onInputResult(sessionId:String,result:InputResult){results.offer(result)}
                    override fun onInputError(sessionId:String)=fail("Input channel failed")
                });media.set(rtc);rtc.applyRemoteOffer(offer)
            }}
            override fun onRemoteIceCandidate(host:ControlledHostSummary,candidate:IceCandidate){media.get()?.addRemoteIceCandidate(candidate)}
            override fun onHostVideoStream(host:ControlledHostSummary,event:HostVideoStreamEvent)=Unit
            override fun onHostTransportStatus(host:ControlledHostSummary,event:HostTransportStatusEvent)=Unit
            override fun onSessionClosed(host:ControlledHostSummary,sessionId:String,reason:String){stopped.countDown()}
            override fun onFailure(host:ControlledHostSummary,message:String)=fail(message)
        })
        val latency=mutableListOf<Long>()
        var expected=0L
        fun command(send:()->Boolean){
            val start=System.nanoTime()
            instrumentation.runOnMainSync { assertTrue("Input send failed: ${error.get()}",send()) }
            val result=results.poll(5,TimeUnit.SECONDS)
            assertNotNull("Missing input result: ${error.get()}",result)
            assertEquals(++expected,result!!.seq);assertFalse("${result.error}",result.hasError())
            latency.add((System.nanoTime()-start)/1_000_000)
        }
        try {
            control.connect();assertTrue(inputReady.await(30,TimeUnit.SECONDS));assertTrue(frame.await(15,TimeUnit.SECONDS));assertNull(error.get())
            val input=requireNotNull(media.get()).input
            val x=args.getString("m6X")!!.toFloat();val y=args.getString("m6Y")!!.toFloat()
            command{input.pointer(x,y,PointerButton.POINTER_BUTTON_LEFT,true)}
            command{input.pointer(x+0.02f,y+0.02f,PointerButton.POINTER_BUTTON_LEFT,true)}
            command{input.pointer(x+0.02f,y+0.02f,PointerButton.POINTER_BUTTON_LEFT,false)}
            command{input.pointer(x,y,PointerButton.POINTER_BUTTON_RIGHT,true)}
            command{input.pointer(x,y,PointerButton.POINTER_BUTTON_RIGHT,false)}
            command{input.scroll(0f,1f)};command{input.scroll(1f,0f)}
            command{input.text("AkaDesk中文🙂")}
            command{input.key(0x43,true,ctrl=true)};command{input.key(0x43,false,ctrl=true)}
            command{input.key(0x78,true,ctrl=true)};command{input.release()}
            // Intentionally keep F8+Shift and left button held until host StopSession cleanup.
            command{input.key(0x77,true,shift=true)}
            command{input.pointer(x,y,PointerButton.POINTER_BUTTON_LEFT,true)}
            control.requestStopSession(id.get()!!)
            assertTrue("No host stop confirmation",stopped.await(5,TimeUnit.SECONDS));assertNull(error.get())
            val sorted=latency.sorted()
            instrumentation.sendStatus(0,Bundle().apply {
                putString("m6InputAckMs",latency.toString());putString("m6InputAckMedianMs",sorted[sorted.size/2].toString())
            })
        } finally {
            id.get()?.let(control::requestStopSession)
            instrumentation.runOnMainSync{media.get()?.close()};control.close()
        }
    }
}
