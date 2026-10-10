package top.akadesk.app.controller

/** Independent deadlines: receiving video status must never cancel a stop request. */
internal class RemoteSessionTimeouts(
    private val scheduler: ControlSessionScheduler,
    onRecoveryTimeout: () -> Unit,
    onStopRetry: () -> Unit,
    onStopTimeout: () -> Unit,
) {
    private var recovering = false
    private var stopping = false
    private val recoveryDeadline = Runnable {
        recovering = false
        onRecoveryTimeout()
    }
    private val stopRetry = Runnable { if (stopping) onStopRetry() }
    private val stopDeadline = Runnable {
        stopping = false
        cancelStopRetry()
        onStopTimeout()
    }

    fun beginRecovery() {
        if (recovering) return
        recovering = true
        scheduler.schedule(recoveryDeadline, 30_000L)
    }

    fun finishRecovery() {
        recovering = false
        scheduler.cancel(recoveryDeadline)
    }

    fun beginStop() {
        if (stopping) return
        stopping = true
        scheduler.schedule(stopDeadline, 15_000L)
    }

    fun awaitStopConfirmation() {
        if (!stopping) return
        cancelStopRetry()
        scheduler.schedule(stopRetry, 3_000L)
    }

    fun cancelStopRetry() {
        scheduler.cancel(stopRetry)
    }

    fun finishStop() {
        stopping = false
        cancelStopRetry()
        scheduler.cancel(stopDeadline)
    }
}
