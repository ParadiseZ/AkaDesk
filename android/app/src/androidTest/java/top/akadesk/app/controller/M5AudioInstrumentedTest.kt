package top.akadesk.app.controller

import akadesk.v1.*
import android.media.AudioManager
import android.util.Log
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

@RunWith(AndroidJUnit4::class)
class M5AudioInstrumentedTest {
    @Test fun windowsSystemAudioArrivesAndMuteControlsRestoreOnDisconnect() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val args = InstrumentationRegistry.getArguments()
        assumeTrue("Requires the isolated Windows M5 fixture", args.containsKey("m5HostId"))
        val context = instrumentation.targetContext
        val manager = context.getSystemService(AudioManager::class.java)
        val originalMode = manager.mode
        val originalVolume = manager.getStreamVolume(AudioManager.STREAM_VOICE_CALL)
        val audioReady = CountDownLatch(1)
        val stopped = CountDownLatch(1)
        val error = AtomicReference<String?>(null)
        val currentSessionId = AtomicReference<String?>(null)
        val media = AtomicReference<AndroidWebRtcSession?>(null)
        val muteResult = AtomicReference<CountDownLatch?>(null)
        val actualMute = AtomicReference<Boolean?>(null)
        val host = ControlledHostSummary("m5-fixture", args.getString("m5HostId")!!, "M5 test", "windows",
            "fixture", "test", args.getString("m5Address", "127.0.0.1")!!, 46671, false, true, 0L)
        lateinit var control: HostControlSession
        fun fail(message: String) { error.compareAndSet(null, message); audioReady.countDown(); muteResult.get()?.countDown() }
        control = HostControlSession(host, "m5-android-test", null, object : HostControlSession.Listener {
            override fun onAuthenticated(host: ControlledHostSummary) = Unit
            override fun onPairingRequired(host: ControlledHostSummary, message: String, errorCode: String) { control.requestPairing("M5 test") }
            override fun onPairingChallenge(host: ControlledHostSummary, challengeId: String, challengeType: PairingChallengeType) { control.confirmPairing(args.getString("m5PairCode")!!) }
            override fun onPairingSucceeded(host: ControlledHostSummary, pairToken: String) = Unit
            override fun onDeviceSnapshotLoaded(host: ControlledHostSummary) { control.requestRemoteSession(VideoCodec.VIDEO_CODEC_H264) }
            override fun onSessionAccepted(host: ControlledHostSummary, sessionId: String, stateLabel: String) { currentSessionId.set(sessionId) }
            override fun onSessionChanged(host: ControlledHostSummary, sessionId: String, stateLabel: String, summary: String) = Unit
            override fun onWebRtcOffer(host: ControlledHostSummary, offer: WebRtcOffer) {
                instrumentation.runOnMainSync {
                    val rtc = AndroidWebRtcSession(context, offer.sessionId, object : AndroidWebRtcSession.Callbacks {
                        override fun onStateChanged(sessionId: String, detail: String) { Log.i("AkaDeskAudioTest", detail) }
                        override fun onLocalAnswer(sessionId: String, sdp: String) { control.sendWebRtcAnswer(sessionId, sdp) }
                        override fun onLocalIceCandidate(sessionId: String, candidate: String, sdpMid: String, sdpMlineIndex: Int) { control.sendIceCandidate(sessionId, candidate, sdpMid, sdpMlineIndex) }
                        override fun onRemoteAudioTrack(sessionId: String) { audioReady.countDown() }
                        override fun onRemoteVideoTrack(sessionId: String) = Unit
                        override fun onRemoteVideoFirstFrame(sessionId: String, width: Int, height: Int) = Unit
                        override fun onRemoteVideoStalled(sessionId: String, stalledForMs: Long, frameCount: Int) = Unit
                        override fun onRemoteVideoFlowResumed(sessionId: String, width: Int, height: Int, frameCount: Int) = Unit
                        override fun onFailure(sessionId: String, message: String) = fail(message)
                    })
                    media.set(rtc)
                    rtc.applyRemoteOffer(offer)
                }
            }
            override fun onRemoteIceCandidate(host: ControlledHostSummary, candidate: IceCandidate) { media.get()?.addRemoteIceCandidate(candidate) }
            override fun onHostVideoStream(host: ControlledHostSummary, event: HostVideoStreamEvent) = Unit
            override fun onHostTransportStatus(host: ControlledHostSummary, event: HostTransportStatusEvent) = Unit
            override fun onSessionClosed(host: ControlledHostSummary, sessionId: String, reason: String) { stopped.countDown() }
            override fun onFailure(host: ControlledHostSummary, message: String) = fail(message)
            override fun onAudioMuteResult(host: ControlledHostSummary, sessionId: String, muted: Boolean, error: String?) {
                actualMute.set(muted)
                if (error != null) fail(error)
                muteResult.get()?.countDown()
            }
        })
        try {
            control.connect()
            assertTrue("No remote audio track: ${error.get()}", audioReady.await(30, TimeUnit.SECONDS))
            assertNull(error.get())
            val rtc = requireNotNull(media.get())
            var received = false
            repeat(15) {
                if (!received) {
                    Thread.sleep(500)
                    val done = CountDownLatch(1)
                    rtc.collectStats { report ->
                        report.statsMap.values.filter { it.type == "inbound-rtp" && (it.members["kind"] == "audio" || it.members["mediaType"] == "audio") }.forEach {
                            Log.i("AkaDeskAudioTest", "audio inbound ${it.members}")
                            instrumentation.sendStatus(0, android.os.Bundle().apply {
                                putString("m5AudioInbound", it.members.toString())
                            })
                            if ((it.members["packetsReceived"] as? Number)?.toLong()?.let { n -> n >= 20 } == true) received = true
                        }
                        done.countDown()
                    }
                    assertTrue(done.await(2, TimeUnit.SECONDS))
                }
            }
            assertTrue("No incoming Opus packets: ${error.get()}", received)
            assertTrue("Android audio output never started", rtc.hasAudioPlayoutStarted())
            // Android stores different volumes for earpiece/speaker/headset routes.
            val playbackVolume = manager.getStreamVolume(AudioManager.STREAM_VOICE_CALL)
            instrumentation.runOnMainSync { rtc.setLocalAudioMuted(true); rtc.setLocalAudioMuted(false) }
            assertEquals(playbackVolume, manager.getStreamVolume(AudioManager.STREAM_VOICE_CALL))
            for (muted in listOf(true, false)) {
                val done = CountDownLatch(1); muteResult.set(done)
                control.setComputerMuted(currentSessionId.get()!!, muted)
                assertTrue(done.await(3, TimeUnit.SECONDS))
                assertNull(error.get())
                assertEquals(muted, actualMute.get())
            }
            control.requestStopSession(currentSessionId.get()!!)
            assertTrue("Host did not confirm stop", stopped.await(5, TimeUnit.SECONDS))
        } finally {
            currentSessionId.get()?.let(control::requestStopSession)
            instrumentation.runOnMainSync { media.get()?.close() }
            control.close()
        }
        assertEquals(originalMode, manager.mode)
        assertEquals(originalVolume, manager.getStreamVolume(AudioManager.STREAM_VOICE_CALL))
    }
}
