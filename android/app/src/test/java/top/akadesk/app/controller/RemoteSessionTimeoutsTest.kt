package top.akadesk.app.controller

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteSessionTimeoutsTest {
    @Test fun restoredVideoDoesNotCancelStopRetryOrStopDeadline() {
        val fixture = Fixture()
        fixture.timeouts.beginRecovery()
        fixture.timeouts.beginStop()
        fixture.timeouts.awaitStopConfirmation()
        fixture.advanceTo(2_000L)
        fixture.timeouts.finishRecovery()
        fixture.advanceTo(30_000L)

        assertEquals(listOf("retry@3000", "stop@15000"), fixture.events)
    }

    @Test fun repeatedReconnectsCannotExtendTheRecoveryDeadline() {
        val fixture = Fixture()
        fixture.timeouts.beginRecovery()
        listOf(1_000L, 3_000L, 7_000L, 15_000L, 29_999L).forEach { time ->
            fixture.advanceTo(time)
            fixture.timeouts.beginRecovery()
        }
        fixture.advanceTo(30_000L)
        assertEquals(listOf("recovery@30000"), fixture.events)
    }

    @Test fun repeatedDisconnectsAndResendsCannotExtendStopDeadline() {
        val fixture = Fixture()
        fixture.timeouts.beginStop()
        fixture.timeouts.awaitStopConfirmation()
        listOf(2_000L, 6_000L, 10_000L, 14_000L).forEach { time ->
            fixture.advanceTo(time)
            fixture.timeouts.beginStop()
            fixture.timeouts.awaitStopConfirmation()
        }
        fixture.advanceTo(20_000L)
        assertEquals(listOf("retry@5000", "retry@9000", "retry@13000", "stop@15000"), fixture.events)
        assertTrue(fixture.pending.isEmpty())
    }

    @Test fun stopConfirmationCancelsAllStopTimersWhileRecoveryRemainsIndependent() {
        val fixture = Fixture()
        fixture.timeouts.beginRecovery()
        fixture.timeouts.beginStop()
        fixture.timeouts.awaitStopConfirmation()
        fixture.advanceTo(1_000L)
        fixture.timeouts.finishStop()
        fixture.advanceTo(30_000L)
        assertEquals(listOf("recovery@30000"), fixture.events)
    }

    @Test fun cleanupPreventsOldTimeoutsFromEndingANewSession() {
        val fixture = Fixture()
        fixture.timeouts.beginRecovery()
        fixture.timeouts.beginStop()
        fixture.advanceTo(5_000L)
        fixture.timeouts.finishRecovery()
        fixture.timeouts.finishStop()
        fixture.advanceTo(6_000L)
        fixture.timeouts.beginRecovery()
        fixture.timeouts.beginStop()
        fixture.advanceTo(36_000L)
        assertEquals(listOf("stop@21000", "recovery@36000"), fixture.events)
    }

    @Test fun connectionLossCancelsResendTimerButPreservesOverallStopDeadline() {
        val fixture = Fixture()
        fixture.timeouts.beginStop()
        fixture.timeouts.awaitStopConfirmation()
        fixture.advanceTo(1_000L)
        fixture.timeouts.cancelStopRetry()
        fixture.advanceTo(4_000L)
        fixture.timeouts.awaitStopConfirmation()
        fixture.advanceTo(15_000L)
        assertEquals(listOf("retry@7000", "stop@15000"), fixture.events)
    }

    private class Fixture : ControlSessionScheduler {
        var now = 0L
        val pending = mutableMapOf<Runnable, Long>()
        val events = mutableListOf<String>()
        val timeouts = RemoteSessionTimeouts(this,
            onRecoveryTimeout = { events += "recovery@$now" },
            onStopRetry = { events += "retry@$now" },
            onStopTimeout = { events += "stop@$now" })
        override fun schedule(task: Runnable, delayMs: Long) { pending[task] = now + delayMs }
        override fun cancel(task: Runnable) { pending.remove(task) }
        fun advanceTo(time: Long) {
            while (true) {
                val entry = pending.entries.minByOrNull { it.value } ?: break
                if (entry.value > time) break
                now = entry.value
                pending.remove(entry.key)
                entry.key.run()
            }
            now = time
        }
    }
}
