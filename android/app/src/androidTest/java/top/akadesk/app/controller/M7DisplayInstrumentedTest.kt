package top.akadesk.app.controller

import androidx.compose.ui.test.*
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

class M7DisplayInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    @Test fun displayChoicesApplyAndRestoreOverAnActualVideoSession() {
        val args = InstrumentationRegistry.getArguments()
        assumeTrue("Requires isolated M7 Windows fixture", args.containsKey("m7HostId"))
        val coordinator = ViewModelProvider(compose.activity)[ControllerViewModel::class.java].coordinator
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val originalCodec = coordinator.preferredVideoCodec
        val frameChecks = mutableListOf<String>()
        val host = ControlledHostSummary("m7-fixture", args.getString("m7HostId")!!, "M7 display test", "windows", "fixture", "test", "127.0.0.1", 46671, false, true, 0L)
        fun decodedFrames(): Long {
            val result = LinkedBlockingQueue<Long>()
            instrumentation.runOnMainSync {
                coordinator.collectRemoteMediaStats { report ->
                    result.offer(report.statsMap.values.filter { it.type == "inbound-rtp" && (it.members["kind"] == "video" || it.members["mediaType"] == "video") }
                        .sumOf { (it.members["framesDecoded"] as? Number)?.toLong() ?: 0L })
                }
            }
            return result.poll(5, TimeUnit.SECONDS) ?: error("Missing video statistics")
        }
        fun verifyFramesContinue(before: Long) {
            val deadline = System.nanoTime() + 8_000_000_000L
            var after = decodedFrames()
            while (after < before + 3 && System.nanoTime() < deadline) { Thread.sleep(150); after = decodedFrames() }
            assertTrue("Video stopped decoding after display change: $before -> $after", after >= before + 3)
            frameChecks += "$before->$after"
        }
        fun refresh() {
            instrumentation.runOnMainSync { coordinator.refreshDisplaySettings() }
            compose.waitUntil(20_000) { !coordinator.displaySettingsPending }
            instrumentation.runOnMainSync { assertNull(coordinator.displaySettingsError); assertNotNull(coordinator.displaySettings) }
        }
        fun apply(mode: akadesk.v1.DisplayMode) {
            val before = decodedFrames()
            instrumentation.runOnMainSync { coordinator.applyDisplaySettings(mode) }
            compose.waitUntil(20_000) { !coordinator.displaySettingsPending }
            instrumentation.runOnMainSync {
                assertNull(coordinator.displaySettingsError)
                assertEquals(mode, coordinator.displaySettings!!.currentMode)
            }
            verifyFramesContinue(before)
        }
        try {
            compose.runOnIdle {
                coordinator.updatePreferredVideoCodec(ControllerVideoCodecPreset.H264)
                coordinator.openVerificationSheet(host, args.getString("m7PairCode")!!)
                coordinator.submitVerificationConnect()
            }
            compose.waitUntil(35_000) { coordinator.remoteInputReady && coordinator.hasActiveRemoteVideoTrack(coordinator.activeSession?.sessionId) }
            compose.onNodeWithText("操作").performClick()
            compose.onNodeWithText("显示",substring=false).performScrollTo().performClick()
            compose.waitUntil(20_000) { !coordinator.displaySettingsPending && coordinator.displaySettings != null }
            val snapshot = coordinator.displaySettings!!
            val original = snapshot.historyDisplayMode
            assertEquals(original, snapshot.currentMode)
            val alternateScale = snapshot.supportedScalePercentsList.first { it != original.scalePercent }
            val beforeScale = decodedFrames()
            // Exercise the production dropdown and apply button first.
            compose.onNodeWithText("缩放倍率：${original.scalePercent}%").performScrollTo().performClick()
            compose.onNodeWithText("$alternateScale%").performClick()
            compose.onNodeWithText("应用到显示器").performScrollTo().performClick()
            compose.waitUntil(20_000) { !coordinator.displaySettingsPending }
            instrumentation.runOnMainSync {
                assertNull(coordinator.displaySettingsError)
                assertEquals(alternateScale, coordinator.displaySettings!!.currentMode.scalePercent)
            }
            verifyFramesContinue(beforeScale)
            val resolution = snapshot.supportedModesList.filter { it.width >= 1280 && it.height >= 720 && it.width <= original.width && it.height <= original.height
                && (it.width != original.width || it.height != original.height) && it.refreshRate == original.refreshRate }
                .maxBy { it.width * it.height }.toBuilder().setScalePercent(original.scalePercent).build()
            apply(resolution)
            val rate = snapshot.supportedModesList.filter { it.width == original.width && it.height == original.height && it.refreshRate != original.refreshRate }
                .minBy { kotlin.math.abs(it.refreshRate - original.refreshRate) }.toBuilder().setScalePercent(original.scalePercent).build()
            apply(rate)
            refresh()
            assertEquals(original, coordinator.displaySettings!!.historyDisplayMode)
            // Restore from the real panel, then leave one change for StopSession recovery.
            compose.onNodeWithText("恢复连接前设置").performScrollTo().performClick()
            compose.waitUntil(20_000) { !coordinator.displaySettingsPending }
            assertEquals(original, coordinator.displaySettings!!.currentMode)
            apply(original.toBuilder().setScalePercent(alternateScale).build())
            instrumentation.runOnMainSync { coordinator.endRemoteSession() }
            compose.waitUntil(15_000) { coordinator.activeSession == null }
            instrumentation.sendStatus(0, android.os.Bundle().apply { putString("m7VideoDecodedFrames", frameChecks.toString()) })
        } finally {
            instrumentation.runOnMainSync {
                coordinator.endRemoteSession()
                coordinator.updatePreferredVideoCodec(originalCodec)
                coordinator.removeHistory(host.deviceId)
                coordinator.pairedDevices.firstOrNull { it.deviceId == host.deviceId }?.let(coordinator::removePairedDevice)
                coordinator.stopDiscovery()
            }
        }
    }
}
