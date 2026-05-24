package top.akadesk.app.controller

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

class ControllerCoordinator(context: Context) {
    private val appContext = context.applicationContext
    private val mainHandler = Handler(Looper.getMainLooper())
    private val pairingStore = ControllerPairingStore(appContext)
    private val controllerDeviceId = pairingStore.controllerDeviceId()

    private var discoveryClient: HostDiscoveryClient? = null
    private var controlSession: HostControlSession? = null
    private var pendingSessionTarget: ControlledHostSummary? = null
    private var pendingConnectMode: ControllerConnectMode? = null
    private var pendingVerificationCode: String? = null
    private var shouldAutoStartSessionAfterAuth = false

    val hosts = androidx.compose.runtime.mutableStateListOf<ControlledHostSummary>()
    val pairedDevices = androidx.compose.runtime.mutableStateListOf<ControllerPairRecord>()
    val favoriteDevices = androidx.compose.runtime.mutableStateListOf<ControllerPairRecord>()
    val historyDevices = androidx.compose.runtime.mutableStateListOf<ControllerPairRecord>()

    var selectedHostForActions by mutableStateOf<ControlledHostSummary?>(null)
    var verificationTarget by mutableStateOf<ControlledHostSummary?>(null)
    var verificationCode by mutableStateOf("")
    var waitingApprovalTarget by mutableStateOf<ControlledHostSummary?>(null)
    var waitingApprovalStartedAtMs by mutableStateOf(0L)
    var remoteAssistDeviceId by mutableStateOf("")
    var preferredVideoCodec by mutableStateOf(pairingStore.preferredVideoCodec())
    var activeSession by mutableStateOf<ActiveRemoteSession?>(null)
    var disconnectNotice by mutableStateOf<ControllerDisconnectNotice?>(null)

    init {
        refreshSavedLists()
    }

    fun startDiscovery() {
        if (discoveryClient == null) {
            discoveryClient = HostDiscoveryClient(
                context = appContext,
                onHostResolved = { host ->
                    mainHandler.post {
                        if (pairingStore.hasPairToken(host.deviceId)) {
                            pairingStore.refreshPairRecord(
                                deviceId = host.deviceId,
                                displayName = host.displayName,
                                hostAddress = host.hostAddress,
                                platform = host.platform,
                            )
                        }
                        if (pairingStore.isFavorite(host.deviceId)) {
                            pairingStore.addFavorite(host)
                        }
                        refreshSavedLists()
                        upsertHost(host)
                        logInfo("Discovered ${host.displayName} ${host.hostAddress}:${host.controlPort}")
                    }
                },
                onError = { message ->
                    mainHandler.post { logWarn(message) }
                },
            )
        }
        hosts.clear()
        logInfo("Start discovery via mDNS")
        discoveryClient?.start()
    }

    fun stopDiscovery() {
        discoveryClient?.stop()
        controlSession?.close()
    }

    fun openHostActions(host: ControlledHostSummary) {
        selectedHostForActions = host
        verificationTarget = null
        waitingApprovalTarget = null
    }

    fun dismissSheets() {
        selectedHostForActions = null
        verificationTarget = null
        waitingApprovalTarget = null
    }

    fun openVerificationSheet(host: ControlledHostSummary, prefillCode: String = "") {
        selectedHostForActions = null
        waitingApprovalTarget = null
        verificationTarget = host
        verificationCode = prefillCode
    }

    fun openWaitingApprovalSheet(host: ControlledHostSummary) {
        selectedHostForActions = null
        verificationTarget = null
        waitingApprovalTarget = host
        waitingApprovalStartedAtMs = System.currentTimeMillis()
        logInfo("Waiting approval for ${host.displayName}")
    }

    fun cancelWaitingApprovalSheet() {
        waitingApprovalTarget = null
    }

    fun connectRememberedHost(host: ControlledHostSummary) {
        dismissSheets()
        beginConnection(host, ControllerConnectMode.TrustedRemembered, verificationCode = null)
    }

    fun submitVerificationConnect() {
        val host = verificationTarget ?: return
        val code = verificationCode.trim().uppercase()
        if (code.isBlank()) {
            logWarn("Verification code is empty")
            return
        }
        dismissSheets()
        beginConnection(host, ControllerConnectMode.VerificationCode, verificationCode = code)
    }

    fun beginRemoteAssistFromInput() {
        val deviceId = remoteAssistDeviceId.trim()
        if (deviceId.isBlank()) {
            logWarn("Remote device id is empty")
            return
        }
        val host = hosts.firstOrNull { it.deviceId == deviceId }
            ?: favoriteDevices.firstOrNull { it.deviceId == deviceId }?.toHostSummary()
            ?: historyDevices.firstOrNull { it.deviceId == deviceId }?.toHostSummary()
            ?: pairedDevices.firstOrNull { it.deviceId == deviceId }?.toHostSummary()

        if (host == null) {
            logWarn("Target device $deviceId not found in current lists")
            return
        }
        if (pairingStore.hasPairToken(host.deviceId)) {
            connectRememberedHost(host)
        } else {
            openVerificationSheet(host)
        }
    }

    fun openApprovalReserveFromInput() {
        val deviceId = remoteAssistDeviceId.trim()
        if (deviceId.isBlank()) {
            logWarn("Remote device id is empty for approval reserve")
            return
        }
        val host = hosts.firstOrNull { it.deviceId == deviceId } ?: return
        openWaitingApprovalSheet(host)
    }

    fun updatePreferredVideoCodec(preset: ControllerVideoCodecPreset) {
        preferredVideoCodec = preset
        pairingStore.savePreferredVideoCodec(preset)
        logInfo("Preferred video codec updated to ${preset.storageKey}")
    }

    fun addFavorite(host: ControlledHostSummary) {
        pairingStore.addFavorite(host)
        refreshSavedLists()
        logInfo("Added favorite ${host.deviceId}")
    }

    fun removeFavorite(deviceId: String) {
        pairingStore.removeFavorite(deviceId)
        refreshSavedLists()
        logInfo("Removed favorite $deviceId")
    }

    fun removeHistory(deviceId: String) {
        pairingStore.removeHistory(deviceId)
        refreshSavedLists()
        logInfo("Removed history $deviceId")
    }

    fun removePairedDevice(record: ControllerPairRecord) {
        pairingStore.removePairing(record.deviceId)
        refreshSavedLists()
        hosts.indexOfFirst { it.deviceId == record.deviceId }
            .takeIf { it >= 0 }
            ?.let { index -> hosts[index] = hosts[index].copy(authState = HostAuthState.Unknown) }
        logInfo("Removed pairing ${record.deviceId}")
    }

    fun removeFoundHost(deviceId: String) {
        hosts.removeAll { it.deviceId == deviceId }
        logInfo("Removed found host $deviceId from current list")
    }

    fun dismissDisconnectNotice() {
        disconnectNotice = null
    }

    fun endRemoteSession() {
        val hostName = activeSession?.host?.displayName ?: pendingSessionTarget?.displayName ?: "当前设备"
        controlSession?.close()
        controlSession = null
        activeSession = null
        pendingSessionTarget = null
        pendingConnectMode = null
        pendingVerificationCode = null
        shouldAutoStartSessionAfterAuth = false
        disconnectNotice = ControllerDisconnectNotice(
            title = "远程控制已断开",
            message = "当前会话已经结束",
            detail = hostName,
        )
        logInfo("Remote session ended for $hostName")
    }

    private fun beginConnection(
        host: ControlledHostSummary,
        connectMode: ControllerConnectMode,
        verificationCode: String?,
    ) {
        disconnectNotice = null
        pendingConnectMode = connectMode
        pendingVerificationCode = verificationCode
        pendingSessionTarget = host
        shouldAutoStartSessionAfterAuth = connectMode != ControllerConnectMode.AwaitRemoteApproval
        remoteAssistDeviceId = host.deviceId
        pairingStore.addHistory(host)
        refreshSavedLists()
        activeSession = ActiveRemoteSession(
            host = host,
            stateLabel = "NEGOTIATING",
            summary = connectMode.description,
        )
        connect(host)
    }

    private fun connect(host: ControlledHostSummary) {
        controlSession?.close()
        logInfo("Connecting to ${host.displayName}")
        controlSession = HostControlSession(
            host = host,
            controllerDeviceId = controllerDeviceId,
            pairToken = pairingStore.pairToken(host.deviceId),
            listener = object : HostControlSession.Listener {
                override fun onAuthenticated(host: ControlledHostSummary) {
                    mainHandler.post {
                        val authenticated = host.copy(authState = HostAuthState.Authenticated)
                        pairingStore.refreshPairRecord(
                            deviceId = host.deviceId,
                            displayName = host.displayName,
                            hostAddress = host.hostAddress,
                            platform = host.platform,
                        )
                        refreshSavedLists()
                        upsertHost(authenticated)
                        logInfo("Authenticated ${host.displayName}")
                        if (shouldAutoStartSessionAfterAuth && pendingSessionTarget?.deviceId == host.deviceId) {
                            requestRemoteSession(authenticated)
                        } else {
                            activeSession = null
                        }
                    }
                }

                override fun onPairingRequired(host: ControlledHostSummary, message: String) {
                    mainHandler.post {
                        val target = host.copy(authState = HostAuthState.PairingRequired)
                        upsertHost(target)
                        logInfo("Pairing required for ${host.displayName}: $message")
                        if (pendingConnectMode == ControllerConnectMode.TrustedRemembered) {
                            pendingConnectMode = ControllerConnectMode.VerificationCode
                            pendingVerificationCode = null
                            activeSession = null
                            openVerificationSheet(target)
                            return@post
                        }
                        controlSession?.requestPairing("AkaDesk Android")
                    }
                }

                override fun onPairingChallenge(host: ControlledHostSummary, challengeId: String) {
                    mainHandler.post {
                        val target = host.copy(authState = HostAuthState.PairingPending)
                        upsertHost(target)
                        val pendingCode = pendingVerificationCode
                        logInfo("Pairing challenge received for ${host.displayName}")
                        if (!pendingCode.isNullOrBlank()) {
                            controlSession?.confirmPairing(pendingCode)
                        } else {
                            activeSession = null
                            openVerificationSheet(target)
                        }
                    }
                }

                override fun onPairingSucceeded(host: ControlledHostSummary, pairToken: String) {
                    mainHandler.post {
                        pairingStore.savePairToken(
                            deviceId = host.deviceId,
                            pairToken = pairToken,
                            displayName = host.displayName,
                            hostAddress = host.hostAddress,
                            platform = host.platform,
                        )
                        refreshSavedLists()
                        logInfo("Pairing succeeded for ${host.displayName}")
                        controlSession?.close()
                        connect(host)
                    }
                }

                override fun onDeviceSnapshotLoaded(host: ControlledHostSummary) {
                    mainHandler.post {
                        logInfo("Device snapshot loaded for ${host.displayName}")
                    }
                }

                override fun onSessionAccepted(host: ControlledHostSummary, sessionId: String) {
                    mainHandler.post {
                        activeSession = ActiveRemoteSession(
                            host = host,
                            sessionId = sessionId,
                            stateLabel = "PREPARING",
                            summary = "正在准备",
                        )
                        logInfo("Session accepted $sessionId for ${host.displayName}")
                    }
                }

                override fun onSessionChanged(
                    host: ControlledHostSummary,
                    sessionId: String,
                    stateLabel: String,
                    summary: String,
                ) {
                    mainHandler.post {
                        activeSession = ActiveRemoteSession(
                            host = host,
                            sessionId = sessionId,
                            stateLabel = stateLabel,
                            summary = summary,
                            startedAtMs = activeSession?.startedAtMs ?: System.currentTimeMillis(),
                        )
                        logInfo("Session changed $sessionId state=$stateLabel summary=$summary")
                    }
                }

                override fun onFailure(host: ControlledHostSummary, message: String) {
                    mainHandler.post {
                        upsertHost(host.copy(authState = HostAuthState.Failed))
                        activeSession = null
                        pendingSessionTarget = null
                        shouldAutoStartSessionAfterAuth = false
                        pendingVerificationCode = null
                        disconnectNotice = ControllerDisconnectNotice(
                            title = "远程控制已断开",
                            message = "当前连接未继续",
                            detail = message,
                        )
                        logError("Connection failure for ${host.displayName}: $message")
                    }
                }
            },
        )
        controlSession?.connect()
    }

    private fun requestRemoteSession(host: ControlledHostSummary) {
        activeSession = ActiveRemoteSession(
            host = host,
            stateLabel = "NEGOTIATING",
            summary = "正在发起画面",
        )
        logInfo("Request remote session ${host.displayName} codec=${preferredVideoCodec.storageKey}")
        controlSession?.requestRemoteSession(preferredVideoCodec.wireCodec)
    }

    private fun refreshSavedLists() {
        pairedDevices.clear()
        pairedDevices += pairingStore.pairedDevices()
        favoriteDevices.clear()
        favoriteDevices += pairingStore.favoriteDevices()
        historyDevices.clear()
        historyDevices += pairingStore.historyDevices()
    }

    private fun upsertHost(host: ControlledHostSummary) {
        val index = hosts.indexOfFirst { it.deviceId == host.deviceId }
        if (index >= 0) {
            hosts[index] = host
        } else {
            hosts += host
        }
    }

    private fun ControllerPairRecord.toHostSummary(): ControlledHostSummary {
        return ControlledHostSummary(
            serviceName = displayName.ifBlank { deviceId },
            deviceId = deviceId,
            displayName = displayName,
            platform = platform,
            hostname = "",
            appVersion = "",
            hostAddress = hostAddress,
            controlPort = 46661,
            pairedAdvertised = pairingStore.hasPairToken(deviceId),
            reachable = false,
            lastSeenMs = savedAtMs,
            authState = if (pairingStore.hasPairToken(deviceId)) HostAuthState.Authenticated else HostAuthState.Unknown,
        )
    }

    private fun logInfo(message: String) {
        Log.i(TAG, message)
    }

    private fun logWarn(message: String) {
        Log.w(TAG, message)
    }

    private fun logError(message: String) {
        Log.e(TAG, message)
    }

    companion object {
        private const val TAG = "AkaDeskController"
    }
}
