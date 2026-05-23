package top.akadesk.app.controller

import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

class ControllerCoordinator(context: Context) {
    private val appContext = context.applicationContext
    private val mainHandler = Handler(Looper.getMainLooper())
    private val pairingStore = ControllerPairingStore(appContext)
    private val controllerDeviceId = pairingStore.controllerDeviceId()

    private var discoveryClient: HostDiscoveryClient? = null
    private var controlSession: HostControlSession? = null

    val hosts = mutableStateListOf<ControlledHostSummary>()
    var statusMessage by mutableStateOf("Ready to scan for Windows hosts.")
    var pairingCode by mutableStateOf("")
    var pairingTarget by mutableStateOf<ControlledHostSummary?>(null)
    var authenticatedTarget by mutableStateOf<ControlledHostSummary?>(null)

    fun startDiscovery() {
        if (discoveryClient == null) {
            discoveryClient = HostDiscoveryClient(
                context = appContext,
                onHostResolved = { host ->
                    mainHandler.post {
                        upsertHost(host)
                        statusMessage = "Discovered ${host.displayName} at ${host.hostAddress}:${host.controlPort}"
                    }
                },
                onError = { message ->
                    mainHandler.post { statusMessage = message }
                }
            )
        }
        hosts.clear()
        statusMessage = "Scanning local network via mDNS..."
        discoveryClient?.start()
    }

    fun stopDiscovery() {
        discoveryClient?.stop()
        controlSession?.close()
    }

    fun connect(host: ControlledHostSummary) {
        controlSession?.close()
        statusMessage = "Connecting to ${host.displayName}..."
        controlSession = HostControlSession(
            host = host,
            controllerDeviceId = controllerDeviceId,
            pairToken = pairingStore.pairToken(host.deviceId),
            listener = object : HostControlSession.Listener {
                override fun onAuthenticated(host: ControlledHostSummary) {
                    mainHandler.post {
                        val authenticated = host.copy(authState = HostAuthState.Authenticated)
                        authenticatedTarget = authenticated
                        pairingTarget = null
                        upsertHost(authenticated)
                        statusMessage = "Authenticated with ${host.displayName}."
                    }
                }

                override fun onPairingRequired(host: ControlledHostSummary, message: String) {
                    mainHandler.post {
                        val target = host.copy(authState = HostAuthState.PairingRequired)
                        pairingTarget = target
                        upsertHost(target)
                        statusMessage = message
                    }
                }

                override fun onPairingChallenge(host: ControlledHostSummary, challengeId: String) {
                    mainHandler.post {
                        val target = host.copy(authState = HostAuthState.PairingPending)
                        pairingTarget = target
                        upsertHost(target)
                        statusMessage = "Enter the 8-character code shown on the Windows host."
                    }
                }

                override fun onPairingSucceeded(host: ControlledHostSummary, pairToken: String) {
                    mainHandler.post {
                        pairingStore.savePairToken(host.deviceId, pairToken)
                        statusMessage = "Pairing succeeded. Reconnecting with saved token..."
                        controlSession?.close()
                        connect(host)
                    }
                }

                override fun onDeviceSnapshotLoaded(host: ControlledHostSummary) {
                    mainHandler.post {
                        statusMessage = "Loaded authenticated device snapshot for ${host.displayName}."
                    }
                }

                override fun onFailure(host: ControlledHostSummary, message: String) {
                    mainHandler.post {
                        upsertHost(host.copy(authState = HostAuthState.Failed))
                        statusMessage = message
                    }
                }
            }
        )
        controlSession?.connect()
    }

    fun startPairing() {
        val target = pairingTarget ?: return
        statusMessage = "Requesting pairing challenge from ${target.displayName}..."
        controlSession?.requestPairing("AkaDesk Android")
    }

    fun submitPairingCode() {
        if (pairingCode.isBlank()) {
            statusMessage = "Enter the code shown on the host first."
            return
        }
        statusMessage = "Submitting verification code..."
        controlSession?.confirmPairing(pairingCode)
    }

    private fun upsertHost(host: ControlledHostSummary) {
        val index = hosts.indexOfFirst { it.deviceId == host.deviceId }
        if (index >= 0) {
            hosts[index] = host
        } else {
            hosts += host
        }
    }
}
