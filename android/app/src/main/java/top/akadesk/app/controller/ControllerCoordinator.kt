package top.akadesk.app.controller

import akadesk.v1.PairingChallengeType
import akadesk.v1.HostVideoStreamEvent
import akadesk.v1.HostTransportStatusEvent
import akadesk.v1.IceCandidate
import akadesk.v1.WebRtcOffer
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.webrtc.SurfaceViewRenderer

class ControllerCoordinator(context: Context) {
    companion object {
        private const val TAG = "AkaDeskController"
        private const val CONTROL_RECONNECT_BASE_DELAY_MS = 1_000L
        private const val CONTROL_RECONNECT_MAX_DELAY_MS = 8_000L
        private const val SESSION_RECOVERY_TIMEOUT_MS = 3_000L
    }

    private data class PendingLocalAnswer(
        val sessionId: String,
        val sdp: String,
    )

    private data class PendingLocalIceCandidate(
        val sessionId: String,
        val candidate: String,
        val sdpMid: String,
        val sdpMlineIndex: Int,
    )

    private val appContext = context.applicationContext
    private val mainHandler = Handler(Looper.getMainLooper())
    private val pairingStore = ControllerPairingStore(appContext)
    private val inputProfileStore = InputProfileStore(appContext)
    var displaySettings by mutableStateOf<akadesk.v1.DisplaySettingsSnapshot?>(null)
        private set
    var displaySettingsPending by mutableStateOf(false)
        private set
    var displaySettingsError by mutableStateOf<String?>(null)
        private set
    private var displayRequestId: String? = null
    private var displayApplying = false
    private val displayTimeout = Runnable {
        if (displaySettingsPending) {
            displayRequestId = null
            displaySettingsPending = false
            displaySettingsError = "未能确认显示设置，请刷新后查看实际结果"
        }
    }
    fun refreshDisplaySettings() {
        val session = activeSession ?: return
        val sessionId = session.sessionId ?: return
        val control = controlSession ?: return
        if (displaySettingsPending || pendingStopSessionId != null) return
        if (!control.supportsDisplayControl) {
            displaySettingsError = "这台电脑暂不支持调整显示设置"
            return
        }
        displaySettingsError = null
        displaySettingsPending = true
        displayApplying = false
        displayRequestId = control.getDisplaySettings(sessionId)
        mainHandler.postDelayed(displayTimeout, 15_000)
    }
    internal fun collectRemoteMediaStats(callback: (org.webrtc.RTCStatsReport) -> Unit) {
        webRtcSession?.collectStats(callback)
    }
    fun applyDisplaySettings(mode: akadesk.v1.DisplayMode) {
        val session = activeSession ?: return
        val sessionId = session.sessionId ?: return
        val control = controlSession ?: return
        val settings = displaySettings ?: return
        if (displaySettingsPending || pendingStopSessionId != null || !control.supportsDisplayControl) return
        if (settings.supportedModesList.none { it.width == mode.width && it.height == mode.height && it.refreshRate == mode.refreshRate }
            || mode.scalePercent !in settings.supportedScalePercentsList) return
        releaseRemoteInput()
        displaySettingsPending = true
        displaySettingsError = null
        displayApplying = true
        displayRequestId = control.setDisplaySettings(sessionId, settings.display.displayId, mode)
        mainHandler.postDelayed(displayTimeout, 15_000)
    }
    var inputShortcuts by mutableStateOf(inputProfileStore.load())
        private set
    var remoteInputReady by mutableStateOf(false)
        private set
    var inputError by mutableStateOf<String?>(null)
        private set
    private var pointerX = 0.5f
    private var pointerY = 0.5f
    var editingLocalInput = false
    var inputForeground = true
    private fun inputAvailable() = inputForeground && remoteInputReady && activeSession?.stateLabel != "DISCONNECTING" && pendingStopSessionId == null
    fun pointerInput(x:Float, y:Float, pressed:Boolean, moveOnly:Boolean=false) {
        if (!inputAvailable() || activeRemoteVideoStalled) return
        pointerX=x; pointerY=y
        webRtcSession?.input?.pointer(x,y,if(moveOnly) akadesk.v1.PointerButton.POINTER_BUTTON_UNSPECIFIED else akadesk.v1.PointerButton.POINTER_BUTTON_LEFT,pressed)
    }
    fun rightClick() {
        if (!inputAvailable()) return
        webRtcSession?.input?.pointer(pointerX,pointerY,akadesk.v1.PointerButton.POINTER_BUTTON_RIGHT,true)
        webRtcSession?.input?.pointer(pointerX,pointerY,akadesk.v1.PointerButton.POINTER_BUTTON_RIGHT,false)
    }
    fun scrollInput(x:Float, y:Float) { if (inputAvailable()) webRtcSession?.input?.scroll(x,y) }
    fun sendShortcut(value:InputShortcut) {
        if (!inputAvailable() || !InputKeys.valid(value)) return
        webRtcSession?.input?.key(value.key,true,value.ctrl,value.alt,value.shift,value.meta)
        webRtcSession?.input?.key(value.key,false,value.ctrl,value.alt,value.shift,value.meta)
    }
    fun sendText(value:String):Boolean {
        if (!inputAvailable()) return false
        releaseRemoteInput()
        return webRtcSession?.input?.text(value)==true
    }
    fun releaseRemoteInput() { webRtcSession?.input?.release() }
    fun saveInputShortcuts(values:List<InputShortcut>):Boolean {
        if (!inputProfileStore.save(values)) return false
        inputShortcuts=values.toList(); return true
    }
    fun hardwareKey(event:android.view.KeyEvent):Boolean {
        if (!inputAvailable() || editingLocalInput) return false
        val key=InputKeys.fromAndroid(event.keyCode) ?: return false
        if (event.action !in listOf(android.view.KeyEvent.ACTION_DOWN,android.view.KeyEvent.ACTION_UP)) return false
        return webRtcSession?.input?.key(key,event.action==android.view.KeyEvent.ACTION_DOWN,event.isCtrlPressed,event.isAltPressed,event.isShiftPressed,event.isMetaPressed)==true
    }
    private val controllerDeviceId = pairingStore.controllerDeviceId()

    private var discoveryClient: HostDiscoveryClient? = null
    private var controlSession: HostControlSession? = null
    private var webRtcSession: AndroidWebRtcSession? = null
    private var mediaGeneration = 0L
    private var activeVideoRenderer: SurfaceViewRenderer? = null
    private var pendingSessionTarget: ControlledHostSummary? = null
    private var pendingConnectMode: ControllerConnectMode? = null
    private var pendingVerificationCode: String? = null
    private var shouldAutoStartSessionAfterAuth = false
    private var pendingControlReconnectHost: ControlledHostSummary? = null
    private var controlReconnectAttempt = 0
    private var controlReconnectScheduled = false
    private var pendingStopSessionId: String? = null
    private var pendingRecoveredSessionId: String? = null
    private var pendingRecoveredSessionHost: ControlledHostSummary? = null
    private var pendingLocalAnswer: PendingLocalAnswer? = null
    private val pendingLocalIceCandidates = linkedSetOf<PendingLocalIceCandidate>()
    private val operationTimeouts = RemoteSessionTimeouts(
        scheduler = AndroidControlSessionScheduler(),
        onRecoveryTimeout = recovery@ {
            val host = pendingControlReconnectHost ?: return@recovery
            handlePreservedSessionRecoveryFailure(host, "连接恢复超时，请重新连接", "控制通道未在 30 秒内恢复")
        },
        onStopTimeout = stopping@ {
            if (pendingStopSessionId != activeSession?.sessionId || pendingStopSessionId == null) return@stopping
            clearPendingControlReconnect()
            closeControlSession()
            closeWebRtcSession()
            activeSession = null
            disconnectNotice = ControllerDisconnectNotice(
                title = "已停止查看",
                message = "电脑未确认断开，已退出当前画面",
                detail = "可在电脑上确认屏幕共享是否已停止。",
            )
            logWarn("Stop-session confirmation deadline reached")
        },
        onStopRetry = retry@ {
            val sessionId = pendingStopSessionId ?: return@retry
            val host = activeSession?.host ?: return@retry
            if (activeSession?.sessionId != sessionId) return@retry
            closeControlSession()
            scheduleControlReconnect(host)
            activeSession = activeSession?.copy(summary = "正在重新连接并确认断开")
            logWarn("Stop-session confirmation timed out; reconnecting session=$sessionId")
        },
    )
    private val controlReconnectRunnable = Runnable {
        controlReconnectScheduled = false
        val previousHost = pendingControlReconnectHost ?: return@Runnable
        val host = hosts.firstOrNull {
            it.deviceId == previousHost.deviceId && it.reachable && it.hostAddress.isNotBlank()
        } ?: previousHost
        pendingControlReconnectHost = host
        if (!shouldKeepActiveMediaSession(host)) {
            clearPendingControlReconnect()
            return@Runnable
        }
        logInfo("Reconnecting control channel to ${host.displayName} attempt=${controlReconnectAttempt}")
        connect(host, preserveMedia = true)
    }
    private val sessionRecoveryTimeoutRunnable = Runnable {
        val sessionId = pendingRecoveredSessionId ?: return@Runnable
        val host = pendingRecoveredSessionHost ?: return@Runnable
        if (activeSession?.sessionId != sessionId) {
            clearPendingSessionRecoveryValidation()
            return@Runnable
        }
        clearPendingSessionRecoveryValidation()
        handlePreservedSessionRecoveryFailure(
            host,
            "会话已失效",
            "控制通道恢复后未找到可恢复的活动会话",
        )
    }

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
    var activeVideoAspectRatio by mutableStateOf(16f / 9f)
        private set
    var remoteAudioReady by mutableStateOf(false)
        private set
    var computerAudioAvailable by mutableStateOf(false)
        private set
    var phoneAudioMuted by mutableStateOf(false)
        private set
    var computerAudioMuted by mutableStateOf(false)
        private set
    var audioMutePending by mutableStateOf(false)
        private set
    var audioError by mutableStateOf<String?>(null)
        private set
    private val audioMuteTimeout = Runnable {
        if (audioMutePending) {
            audioMutePending = false
            audioError = "未能确认电脑声音设置，请重试"
        }
    }
    var activeRemoteVideoReady by mutableStateOf(false)
        private set
    var activeRemoteVideoStalled by mutableStateOf(false)
        private set
    var activeHostVideoStatus by mutableStateOf<String?>(null)
        private set
    var activeHostVideoStalled by mutableStateOf(false)
        private set
    var activeHostTransportStatus by mutableStateOf<String?>(null)
        private set
    var activeHostTransportStalled by mutableStateOf(false)
        private set
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
                onHostLost = { serviceName ->
                    mainHandler.post { hosts.removeAll { it.serviceName == serviceName } }
                },
            )
        }
        hosts.clear()
        logInfo("Start discovery via mDNS")
        discoveryClient?.start()
    }

    private var controlGeneration = 0L

    fun stopDiscovery() {
        discoveryClient?.stop()
        clearPendingControlReconnect()
        closeControlSession()
        closeWebRtcSession()
        activeSession = null
    }

    fun openHostActions(host: ControlledHostSummary) {
        selectedHostForActions = host
        verificationTarget = null
        waitingApprovalTarget = null
    }

    fun dismissSheets() {
        if (pendingSessionTarget != null && (verificationTarget != null || waitingApprovalTarget != null)) {
            cancelWaitingApprovalSheet()
        }
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
        clearPendingControlReconnect()
        closeControlSession()
        closeWebRtcSession()
        activeSession = null
        pendingSessionTarget = null
        pendingConnectMode = null
        pendingVerificationCode = null
        shouldAutoStartSessionAfterAuth = false
    }

    fun connectAwaitRemoteApproval(host: ControlledHostSummary) {
        dismissSheets()
        beginConnection(host, ControllerConnectMode.AwaitRemoteApproval, verificationCode = null)
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
        connectAwaitRemoteApproval(host)
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
        releaseRemoteInput()
        val hostName = activeSession?.host?.displayName ?: pendingSessionTarget?.displayName ?: "当前设备"
        val sessionId = activeSession?.sessionId
        val currentControlSession = controlSession
        if (!sessionId.isNullOrBlank()) {
            if (pendingStopSessionId == sessionId) return
            pendingStopSessionId = sessionId
            clearPendingSessionRecoveryValidation()
            operationTimeouts.beginStop()
            if (currentControlSession != null) {
                currentControlSession.requestStopSession(sessionId)
                scheduleStopSessionRetry()
            } else {
                activeSession?.host?.let(::scheduleControlReconnect)
            }
            activeSession = activeSession?.copy(
                stateLabel = "DISCONNECTING",
                summary = "正在请求断开当前远控会话",
            )
            logInfo("Requested remote session stop for $hostName session=$sessionId")
            return
        }
        clearPendingControlReconnect()
        closeControlSession()
        closeWebRtcSession()
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
        shouldAutoStartSessionAfterAuth = true
        remoteAssistDeviceId = host.deviceId
        pairingStore.addHistory(host)
        refreshSavedLists()
        activeSession = ActiveRemoteSession(
            host = host,
            stateLabel = "NEGOTIATING",
            summary = connectMode.description,
        )
        connect(host, preserveMedia = false)
    }

    private fun connect(host: ControlledHostSummary, preserveMedia: Boolean) {
        val generation = ++controlGeneration
        if (!preserveMedia) {
            clearPendingControlReconnect()
            closeWebRtcSession()
        } else {
            clearScheduledControlReconnect()
        }
        controlSession?.close()
        logInfo("Connecting to ${host.displayName}")
        controlSession = HostControlSession(
            host = host,
            controllerDeviceId = controllerDeviceId,
            pairToken = pairingStore.pairToken(host.deviceId),
            listener = object : HostControlSession.Listener {
                override fun onAuthenticated(host: ControlledHostSummary) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        clearScheduledControlReconnect()
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
                        if (preserveMedia) {
                            if (activeSession?.host?.deviceId == host.deviceId) {
                                activeSession = activeSession?.copy(
                                    host = authenticated,
                                    summary = if (pendingStopSessionId == activeSession?.sessionId) {
                                        "控制通道已恢复，正在请求断开当前远控会话"
                                    } else if (activeRemoteVideoReady) {
                                        "控制通道已恢复，当前继续显示被控画面"
                                    } else {
                                        "控制通道已恢复，等待远端媒体继续推进"
                                    },
                                )
                            }
                            if (pendingStopSessionId == activeSession?.sessionId) {
                                controlSession?.requestStopSession(pendingStopSessionId!!)
                                scheduleStopSessionRetry()
                                logInfo("Re-sent stop session request after control reconnect session=$pendingStopSessionId")
                            } else {
                                activeSession?.sessionId?.let { schedulePendingSessionRecoveryValidation(host, it) }
                                resendActiveLocalSignaling()
                            }
                        } else if (shouldAutoStartSessionAfterAuth && pendingSessionTarget?.deviceId == host.deviceId) {
                            requestRemoteSession(authenticated)
                        } else {
                            activeSession = null
                        }
                    }
                }

                override fun onPairingRequired(host: ControlledHostSummary, message: String, errorCode: String) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        if (preserveMedia && activeSession?.host?.deviceId == host.deviceId) {
                            handlePreservedSessionRecoveryFailure(host, "认证已失效", message)
                            return@post
                        }
                        val target = host.copy(authState = HostAuthState.PairingRequired)
                        upsertHost(target)
                        logInfo("Pairing required for ${host.displayName}: $message code=$errorCode")
                        if (errorCode == "AUTH_TRUST_ONLY_CONSENT_REQUIRED") {
                            pendingConnectMode = ControllerConnectMode.AwaitRemoteApproval
                            pendingVerificationCode = null
                            activeSession = null
                            openWaitingApprovalSheet(target)
                            controlSession?.requestPairing("AkaDesk Android")
                            return@post
                        }
                        if (errorCode == "AUTH_ACCESS_CODE_AND_CONSENT_REQUIRED" && pendingVerificationCode.isNullOrBlank()) {
                            pendingConnectMode = ControllerConnectMode.VerificationCode
                            pendingVerificationCode = null
                            activeSession = null
                            openVerificationSheet(target)
                            return@post
                        }
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

                override fun onPairingChallenge(
                    host: ControlledHostSummary,
                    challengeId: String,
                    challengeType: PairingChallengeType,
                ) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        val target = host.copy(authState = HostAuthState.PairingPending)
                        upsertHost(target)
                        logInfo("Pairing challenge received for ${host.displayName} type=${challengeType.name}")
                        if (challengeType == PairingChallengeType.PAIRING_CHALLENGE_TYPE_LOCAL_CONFIRM) {
                            activeSession = null
                            openWaitingApprovalSheet(target)
                            return@post
                        }
                        val pendingCode = pendingVerificationCode
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
                        if (generation != controlGeneration) return@post
                        waitingApprovalTarget = null
                        pairingStore.savePairToken(
                            deviceId = host.deviceId,
                            pairToken = pairToken,
                            displayName = host.displayName,
                            hostAddress = host.hostAddress,
                            platform = host.platform,
                        )
                        refreshSavedLists()
                        logInfo("Pairing succeeded for ${host.displayName}")
                    }
                }

                override fun onDeviceSnapshotLoaded(host: ControlledHostSummary) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        logInfo("Device snapshot loaded for ${host.displayName}")
                    }
                }

                override fun onSessionAccepted(host: ControlledHostSummary, sessionId: String, stateLabel: String) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        if (pendingStopSessionId != null) return@post
                        val currentSessionId = activeSession?.sessionId
                        if (!currentSessionId.isNullOrBlank() &&
                            (currentSessionId != sessionId || webRtcSession != null)) return@post
                        closeWebRtcSession()
                        selectedHostForActions = null
                        verificationTarget = null
                        waitingApprovalTarget = null
                        pendingSessionTarget = null
                        pendingConnectMode = null
                        pendingVerificationCode = null
                        shouldAutoStartSessionAfterAuth = false
                        val summary = when (stateLabel) {
                            "STREAMING" -> "画面传输已启动，等待远端 WebRTC 首帧"
                            "PREPARING" -> "正在准备"
                            else -> "会话已接受，当前状态=$stateLabel"
                        }
                        activeSession = ActiveRemoteSession(
                            host = host,
                            sessionId = sessionId,
                            stateLabel = stateLabel,
                            summary = summary,
                        )
                        ensureWebRtcSession(host, sessionId)
                        logInfo("Session accepted $sessionId state=$stateLabel for ${host.displayName}")
                    }
                }

                override fun onSessionChanged(
                    host: ControlledHostSummary,
                    sessionId: String,
                    stateLabel: String,
                    summary: String,
                ) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        if (isTerminalSessionState(stateLabel)) {
                            if (activeSession?.sessionId != sessionId && pendingStopSessionId != sessionId) return@post
                            clearPendingControlReconnect()
                            closeControlSession()
                            closeWebRtcSession()
                            activeSession = null
                            waitingApprovalTarget = null
                            pendingSessionTarget = null
                            pendingConnectMode = null
                            shouldAutoStartSessionAfterAuth = false
                            pendingVerificationCode = null
                            disconnectNotice = ControllerDisconnectNotice(
                                title = "远程控制已断开",
                                message = "被控端会话已结束",
                                detail = if (stateLabel == "FAILED") "画面连接失败，请重试。" else "电脑已停止屏幕共享。",
                            )
                            logWarn("Session ended $sessionId state=$stateLabel summary=$summary")
                            return@post
                        }
                        val currentSessionId = activeSession?.sessionId
                        if (!currentSessionId.isNullOrBlank() && currentSessionId != sessionId) {
                            logWarn("Ignoring state for stale session=$sessionId, active=$currentSessionId")
                            return@post
                        }
                        if (activeSession?.sessionId != sessionId) {
                            closeWebRtcSession()
                            activeHostVideoStatus = null
                            activeHostVideoStalled = false
                            activeHostTransportStatus = null
                            activeHostTransportStalled = false
                        }
                        if (activeSession?.sessionId == sessionId) {
                            confirmActiveSessionRecovery(sessionId)
                        }
                        activeSession = ActiveRemoteSession(
                            host = host,
                            sessionId = sessionId,
                            stateLabel = if (pendingStopSessionId == sessionId) "DISCONNECTING" else stateLabel,
                            summary = summary,
                            startedAtMs = activeSession?.startedAtMs ?: System.currentTimeMillis(),
                        )
                        logInfo("Session changed $sessionId state=$stateLabel summary=$summary")
                    }
                }

                override fun onAudioState(host: ControlledHostSummary, sessionId: String, available: Boolean, muted: Boolean) {
                    mainHandler.post {
                        if (generation != controlGeneration || activeSession?.sessionId != sessionId) return@post
                        computerAudioAvailable = available
                        computerAudioMuted = muted
                    }
                }

                override fun onDisplaySettings(host: ControlledHostSummary, requestId: String, response: akadesk.v1.DisplaySettingsResponse) {
                    mainHandler.post {
                        if (generation != controlGeneration || response.sessionId != activeSession?.sessionId || requestId != displayRequestId) return@post
                        mainHandler.removeCallbacks(displayTimeout)
                        displaySettingsPending = false
                        displayRequestId = null
                        if (response.hasSettings()) displaySettings = response.settings
                        displaySettingsError = if (!response.hasError()) null else if (displayApplying)
                            "电脑未能应用显示设置，已尝试恢复原设置，请刷新确认"
                        else "未能读取显示设置，请刷新重试"
                        if (response.hasError()) logWarn("Display settings failed: ${response.error.message}")
                    }
                }
                override fun onAudioMuteResult(host: ControlledHostSummary, sessionId: String, muted: Boolean, error: String?) {
                    mainHandler.post {
                        if (generation != controlGeneration || activeSession?.sessionId != sessionId) return@post
                        mainHandler.removeCallbacks(audioMuteTimeout)
                        audioMutePending = false
                        computerAudioMuted = muted
                        audioError = if (error == null) null else "电脑声音设置失败，请重试"
                        if (error != null) logWarn("Computer mute failed: $error")
                    }
                }

                override fun onWebRtcOffer(host: ControlledHostSummary, offer: WebRtcOffer) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        val activeSessionId = activeSession?.sessionId
                        if (activeSessionId.isNullOrBlank()) {
                            logWarn("Ignoring WebRTC offer for inactive session=${offer.sessionId}")
                            return@post
                        }
                        if (activeSessionId != offer.sessionId) {
                            logWarn("Ignoring WebRTC offer for stale session=${offer.sessionId}, active=$activeSessionId")
                            return@post
                        }
                        if (pendingStopSessionId == offer.sessionId) return@post
                        confirmActiveSessionRecovery(offer.sessionId)
                        val session = ensureWebRtcSession(host, offer.sessionId)
                        session.applyRemoteOffer(offer)
                        logInfo("Received WebRTC offer for ${host.displayName} session=${offer.sessionId} sdpLen=${offer.sdp.length}")
                    }
                }

                override fun onRemoteIceCandidate(
                    host: ControlledHostSummary,
                    candidate: IceCandidate,
                ) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        val activeSessionId = activeSession?.sessionId
                        if (activeSessionId.isNullOrBlank()) {
                            logWarn("Ignoring remote ICE candidate for inactive session=${candidate.sessionId}")
                            return@post
                        }
                        if (activeSessionId != candidate.sessionId) {
                            logWarn("Ignoring remote ICE candidate for stale session=${candidate.sessionId}, active=$activeSessionId")
                            return@post
                        }
                        if (pendingStopSessionId == candidate.sessionId) return@post
                        confirmActiveSessionRecovery(candidate.sessionId)
                        val session = ensureWebRtcSession(host, candidate.sessionId)
                        session.addRemoteIceCandidate(candidate)
                        logInfo("Received remote ICE candidate for ${host.displayName} session=${candidate.sessionId}")
                    }
                }

                override fun onHostVideoStream(host: ControlledHostSummary, event: HostVideoStreamEvent) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        if (activeSession?.sessionId != event.sessionId) {
                            return@post
                        }
                        confirmActiveSessionRecovery(event.sessionId)
                        val hostStalled = event.latestError.isNotBlank() ||
                            (System.currentTimeMillis() - event.updatedAtMs) >= 2_500L
                        activeHostVideoStalled = hostStalled
                        activeHostVideoStatus = if (event.latestError.isNotBlank()) {
                            "Host 出包错误: ${event.latestError}"
                        } else if (hostStalled) {
                            "Host 出包疑似停滞，累计 ${event.packetCount} 包 / ${event.keyframeCount} 关键帧"
                        } else {
                            "Host 持续出包 ${event.packetCount} 包 / ${event.keyframeCount} 关键帧 / ${event.latestPacketLen}B / ${event.latestFrameSourceLabel}"
                        }
                        logInfo(
                            "Host video stream event session=${event.sessionId} packets=${event.packetCount} keyframes=${event.keyframeCount} latestError=${event.latestError}"
                        )
                    }
                }

                override fun onHostTransportStatus(host: ControlledHostSummary, event: HostTransportStatusEvent) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        if (activeSession?.sessionId != event.sessionId) {
                            return@post
                        }
                        confirmActiveSessionRecovery(event.sessionId)
                        val transportStalled = event.latestError.isNotBlank() ||
                            event.peerConnectionState.equals("Failed", ignoreCase = true) ||
                            event.peerConnectionState.equals("Disconnected", ignoreCase = true) ||
                            (event.remoteAnswerApplied && event.sentSampleCount == 0L)
                        activeHostTransportStalled = transportStalled
                        activeHostTransportStatus = if (event.latestError.isNotBlank()) {
                            "Transport 错误: ${event.latestError}"
                        } else if (transportStalled) {
                            "Transport 状态异常: peer=${event.peerConnectionState} / sent=${event.sentSampleCount} / remoteAnswer=${event.remoteAnswerApplied}"
                        } else {
                            "Transport 正在发送: peer=${event.peerConnectionState} / sent=${event.sentSampleCount} / remoteAnswer=${event.remoteAnswerApplied} / remoteIce=${event.appliedRemoteCandidateCount}"
                        }
                        logInfo(
                            "Host transport event session=${event.sessionId} peer=${event.peerConnectionState} sent=${event.sentSampleCount} remoteAnswer=${event.remoteAnswerApplied}"
                        )
                    }
                }

                override fun onSessionClosed(host: ControlledHostSummary, sessionId: String, reason: String) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        if (activeSession?.sessionId != sessionId) {
                            return@post
                        }
                        clearPendingSessionRecoveryValidation()
                        clearPendingControlReconnect()
                        closeControlSession()
                        closeWebRtcSession()
                        pendingStopSessionId = null
                        activeSession = null
                        disconnectNotice = ControllerDisconnectNotice(
                            title = "远程控制已断开",
                            message = "当前会话已经结束",
                            detail = if (reason == "CONTROLLER_REQUEST") "已停止查看电脑画面。" else "屏幕共享已结束。",
                        )
                        logInfo("Session closed session=$sessionId reason=$reason")
                    }
                }

                override fun onFailure(host: ControlledHostSummary, message: String) {
                    mainHandler.post {
                        if (generation != controlGeneration) return@post
                        val keepActiveVideoSession = shouldKeepActiveMediaSession(host)
                        clearPendingSessionRecoveryValidation()
                        operationTimeouts.cancelStopRetry()
                        closeControlSession()
                        waitingApprovalTarget = null
                        pendingSessionTarget = null
                        pendingConnectMode = null
                        shouldAutoStartSessionAfterAuth = false
                        pendingVerificationCode = null
                        if (keepActiveVideoSession) {
                            upsertHost(host.copy(authState = HostAuthState.Authenticated))
                            activeSession = activeSession?.copy(
                                summary = "控制通道已断开，正在重连，当前继续保留现有被控画面"
                            )
                            scheduleControlReconnect(host)
                            logWarn("Control channel failed for ${host.displayName}, keeping active media session: $message")
                            return@post
                        }
                        clearPendingControlReconnect()
                        upsertHost(host.copy(authState = HostAuthState.Failed))
                        closeWebRtcSession()
                        activeSession = null
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
        closeWebRtcSession()
        activeSession = ActiveRemoteSession(
            host = host,
            stateLabel = "NEGOTIATING",
            summary = "正在发起画面",
        )
        logInfo("Request remote session ${host.displayName} codec=${preferredVideoCodec.storageKey}")
        controlSession?.requestRemoteSession(preferredVideoCodec.wireCodec)
    }

    fun bindActiveVideoRenderer(renderer: SurfaceViewRenderer) {
        activeVideoRenderer = renderer
        webRtcSession?.bindRenderer(renderer)
    }

    fun releaseActiveVideoRenderer(renderer: SurfaceViewRenderer) {
        if (activeVideoRenderer === renderer) {
            activeVideoRenderer = null
        }
        webRtcSession?.releaseRenderer(renderer) ?: renderer.release()
    }

    fun hasActiveRemoteVideoTrack(sessionId: String?): Boolean {
        if (sessionId.isNullOrBlank()) {
            return false
        }
        return activeSession?.sessionId == sessionId && activeRemoteVideoReady
    }

    fun togglePhoneAudioMute() {
        if (!remoteAudioReady || activeSession?.stateLabel == "DISCONNECTING") return
        phoneAudioMuted = !phoneAudioMuted
        webRtcSession?.setLocalAudioMuted(phoneAudioMuted)
    }

    fun toggleComputerAudioMute() {
        val sessionId = activeSession?.sessionId ?: return
        val control = controlSession ?: return
        if (!computerAudioAvailable || audioMutePending || pendingStopSessionId != null) return
        audioError = null
        audioMutePending = true
        mainHandler.postDelayed(audioMuteTimeout, 3_000)
        control.setComputerMuted(sessionId, !computerAudioMuted)
    }

    fun isActiveRemoteVideoStalled(sessionId: String?): Boolean {
        if (sessionId.isNullOrBlank()) {
            return false
        }
        return activeSession?.sessionId == sessionId && activeRemoteVideoStalled
    }

    fun activeHostVideoStatus(sessionId: String?): String? {
        if (sessionId.isNullOrBlank()) {
            return null
        }
        return if (activeSession?.sessionId == sessionId) activeHostVideoStatus else null
    }

    fun isActiveHostVideoStalled(sessionId: String?): Boolean {
        if (sessionId.isNullOrBlank()) {
            return false
        }
        return activeSession?.sessionId == sessionId && activeHostVideoStalled
    }

    fun activeHostTransportStatus(sessionId: String?): String? {
        if (sessionId.isNullOrBlank()) {
            return null
        }
        return if (activeSession?.sessionId == sessionId) activeHostTransportStatus else null
    }

    fun isActiveHostTransportStalled(sessionId: String?): Boolean {
        if (sessionId.isNullOrBlank()) {
            return false
        }
        return activeSession?.sessionId == sessionId && activeHostTransportStalled
    }

    fun activeRemoteVideoDiagnosis(sessionId: String?): RemoteVideoPathDiagnosis? {
        if (sessionId.isNullOrBlank()) {
            return null
        }
        if (activeSession?.sessionId != sessionId) {
            return null
        }
        return when {
            activeRemoteVideoStalled && activeHostTransportStalled -> RemoteVideoPathDiagnosis(
                title = "Transport 发送已异常",
                detail = "Host transport 已异常且 Android 已停止收到新帧，优先检查 WebRTC 发送链路。",
                level = RemoteVideoDiagnosisLevel.Critical,
            )

            activeRemoteVideoStalled && activeHostVideoStalled -> RemoteVideoPathDiagnosis(
                title = "Host 出包链已停滞",
                detail = "Host 侧编码或出包已经异常，Android 当前更像是停在最后一帧。",
                level = RemoteVideoDiagnosisLevel.Critical,
            )

            activeRemoteVideoStalled -> RemoteVideoPathDiagnosis(
                title = "Android 接收或渲染链疑似停滞",
                detail = "Host 仍在出包并继续写 sample，但 Android 最近没有收到新帧。",
                level = RemoteVideoDiagnosisLevel.Warning,
            )

            !activeRemoteVideoReady && activeHostTransportStalled -> RemoteVideoPathDiagnosis(
                title = "Transport 尚未真正发出首帧",
                detail = "Host transport 已异常或 sample 未增长，Android 仍在等待首帧。",
                level = RemoteVideoDiagnosisLevel.Critical,
            )

            !activeRemoteVideoReady && activeHostVideoStalled -> RemoteVideoPathDiagnosis(
                title = "Host 出包链尚未推到首帧",
                detail = "Host 侧出包已经异常或停滞，Android 还没有收到首帧。",
                level = RemoteVideoDiagnosisLevel.Critical,
            )

            !activeRemoteVideoReady && (activeHostVideoStatus != null || activeHostTransportStatus != null) -> RemoteVideoPathDiagnosis(
                title = "Host 正在推流，Android 等待首帧",
                detail = "Host 已开始出包或发送，当前更可能卡在 Android 解码、渲染或首帧到达。",
                level = RemoteVideoDiagnosisLevel.Info,
            )

            activeRemoteVideoReady && (activeHostVideoStalled || activeHostTransportStalled) -> RemoteVideoPathDiagnosis(
                title = "画面已出现，但 Host 链路有波动",
                detail = "Android 当前仍有画面，但 Host 出包或 transport 状态已经异常，持续性有风险。",
                level = RemoteVideoDiagnosisLevel.Warning,
            )

            activeRemoteVideoReady -> RemoteVideoPathDiagnosis(
                title = "视频链路看起来正常",
                detail = "Host 出包、transport 发送和 Android 收帧目前一致。",
                level = RemoteVideoDiagnosisLevel.Info,
            )

            else -> RemoteVideoPathDiagnosis(
                title = "正在建立视频链路",
                detail = "等待 Host signaling、出包和 Android 首帧继续推进。",
                level = RemoteVideoDiagnosisLevel.Info,
            )
        }
    }

    private fun ensureWebRtcSession(
        host: ControlledHostSummary,
        sessionId: String,
    ): AndroidWebRtcSession {
        val current = webRtcSession
        if (current != null && activeSession?.sessionId == sessionId) {
            return current
        }
        closeWebRtcSession()
        val generation = mediaGeneration
        return AndroidWebRtcSession(
            appContext = appContext,
            sessionId = sessionId,
            callbacks = object : AndroidWebRtcSession.Callbacks {
                override fun onInputStateChanged(sessionId: String, ready: Boolean) {
                    mainHandler.post {
                        if (generation != mediaGeneration || activeSession?.sessionId != sessionId) return@post
                        if (!ready && remoteInputReady && pendingStopSessionId == null) inputError="输入连接已断开，请重新连接"
                        if (ready) inputError=null
                        remoteInputReady=ready
                    }
                }
                override fun onInputError(sessionId: String) {
                    mainHandler.post {
                        if (generation != mediaGeneration || activeSession?.sessionId != sessionId) return@post
                        remoteInputReady=false
                        inputError="电脑暂时无法接收操作，请重新连接"
                    }
                }
                override fun onStateChanged(sessionId: String, detail: String) {
                    mainHandler.post {
                        if (generation != mediaGeneration) return@post
                        if (activeSession?.sessionId == sessionId) {
                            activeSession = activeSession?.copy(summary = detail)
                        }
                        logInfo("WebRTC state changed session=$sessionId detail=$detail")
                    }
                }

                override fun onLocalAnswer(sessionId: String, sdp: String) {
                    mainHandler.post {
                        if (generation != mediaGeneration) return@post
                        if (activeSession?.sessionId != sessionId) return@post
                        pendingLocalAnswer = PendingLocalAnswer(sessionId, sdp)
                        controlSession?.sendWebRtcAnswer(sessionId, sdp)
                        activeSession = activeSession?.copy(summary = "正在等待远端画面")
                        logInfo("Local WebRTC answer ready session=$sessionId")
                    }
                }

                override fun onLocalIceCandidate(
                    sessionId: String,
                    candidate: String,
                    sdpMid: String,
                    sdpMlineIndex: Int,
                ) {
                    mainHandler.post {
                        if (generation != mediaGeneration) return@post
                        if (activeSession?.sessionId != sessionId) return@post
                        pendingLocalIceCandidates += PendingLocalIceCandidate(
                            sessionId = sessionId,
                            candidate = candidate,
                            sdpMid = sdpMid,
                            sdpMlineIndex = sdpMlineIndex,
                        )
                        if (controlSession != null) {
                            controlSession?.sendIceCandidate(sessionId, candidate, sdpMid, sdpMlineIndex)
                            logInfo("Sent local ICE candidate session=$sessionId")
                        } else {
                            logWarn("Queued local ICE candidate session=$sessionId until control reconnect")
                        }
                    }
                }

                override fun onRemoteAudioTrack(sessionId: String) {
                    mainHandler.post {
                        if (generation != mediaGeneration || activeSession?.sessionId != sessionId) return@post
                        remoteAudioReady = true
                        webRtcSession?.setLocalAudioMuted(phoneAudioMuted)
                        logInfo("Remote audio track negotiated session=$sessionId")
                    }
                }

                override fun onRemoteVideoTrack(sessionId: String) {
                    mainHandler.post {
                        if (generation != mediaGeneration) return@post
                        if (activeSession?.sessionId != sessionId) {
                            return@post
                        }
                        activeRemoteVideoReady = false
                        activeRemoteVideoStalled = false
                        activeSession = activeSession?.copy(summary = "已协商远端视频轨道，等待首帧到达")
                        logInfo("Remote video track negotiated for session=$sessionId")
                    }
                }

                override fun onRemoteVideoFirstFrame(sessionId: String, width: Int, height: Int) {
                    mainHandler.post {
                        if (generation != mediaGeneration) return@post
                        if (activeSession?.sessionId != sessionId) {
                            return@post
                        }
                        activeVideoAspectRatio = width.toFloat() / height.coerceAtLeast(1)
                        activeRemoteVideoReady = true
                        activeRemoteVideoStalled = false
                        activeSession = activeSession?.copy(
                            summary = "已收到远端首帧 ${width}x${height}，正在显示被控画面"
                        )
                        logInfo("Remote first video frame received session=$sessionId size=${width}x$height")
                    }
                }

                override fun onRemoteVideoStalled(sessionId: String, stalledForMs: Long, frameCount: Int) {
                    mainHandler.post {
                        if (generation != mediaGeneration) return@post
                        if (activeSession?.sessionId != sessionId) {
                            return@post
                        }
                        activeRemoteVideoStalled = true
                        releaseRemoteInput()
                        activeSession = activeSession?.copy(
                            summary = "远端视频流疑似停滞，${stalledForMs}ms 未收到新帧（累计 ${frameCount} 帧）"
                        )
                        logWarn("Remote video stalled session=$sessionId stalledForMs=$stalledForMs frameCount=$frameCount")
                    }
                }

                override fun onRemoteVideoFlowResumed(sessionId: String, width: Int, height: Int, frameCount: Int) {
                    mainHandler.post {
                        if (generation != mediaGeneration) return@post
                        if (activeSession?.sessionId != sessionId) {
                            return@post
                        }
                        activeVideoAspectRatio = width.toFloat() / height.coerceAtLeast(1)
                        activeRemoteVideoReady = true
                        activeRemoteVideoStalled = false
                        activeSession = activeSession?.copy(
                            summary = "远端视频流已恢复 ${width}x${height}，累计 ${frameCount} 帧"
                        )
                        logInfo("Remote video resumed session=$sessionId size=${width}x$height frameCount=$frameCount")
                    }
                }

                override fun onFailure(sessionId: String, message: String) {
                    mainHandler.post {
                        if (generation != mediaGeneration) return@post
                        if (activeSession?.sessionId != sessionId) {
                            return@post
                        }
                        closeWebRtcSession(retainPendingStop = true)
                        activeSession = activeSession?.copy(summary = "画面连接失败，正在结束查看")
                        endRemoteSession()
                        logError("Android WebRTC failure session=$sessionId message=$message")
                    }
                }
            },
        ).also { created ->
            webRtcSession = created
            activeVideoRenderer?.let(created::bindRenderer)
            if (activeSession?.sessionId == sessionId) {
                activeSession = activeSession?.copy(
                    host = host,
                    summary = "Android WebRTC 已创建，等待远端 Offer",
                )
            }
        }
    }

    private fun closeWebRtcSession(retainPendingStop: Boolean = false) {
        mainHandler.removeCallbacks(displayTimeout)
        displaySettings = null
        displaySettingsPending = false
        displaySettingsError = null
        displayRequestId = null
        mediaGeneration++
        webRtcSession?.close()
        webRtcSession = null
        remoteInputReady=false
        inputError=null
        pointerX=0.5f; pointerY=0.5f
        mainHandler.removeCallbacks(audioMuteTimeout)
        remoteAudioReady = false
        computerAudioAvailable = false
        phoneAudioMuted = false
        computerAudioMuted = false
        audioMutePending = false
        audioError = null
        clearPendingSessionRecoveryValidation()
        if (!retainPendingStop) {
            pendingStopSessionId = null
            operationTimeouts.finishStop()
        }
        pendingLocalAnswer = null
        pendingLocalIceCandidates.clear()
        activeRemoteVideoReady = false
        activeRemoteVideoStalled = false
        activeHostVideoStatus = null
        activeHostVideoStalled = false
        activeHostTransportStatus = null
        activeHostTransportStalled = false
    }

    private fun resendActiveLocalSignaling() {
        val sessionId = activeSession?.sessionId
        if (sessionId.isNullOrBlank()) {
            return
        }
        val answer = pendingLocalAnswer
        if (answer != null && answer.sessionId == sessionId) {
            controlSession?.sendWebRtcAnswer(answer.sessionId, answer.sdp)
            logInfo("Re-sent local WebRTC answer session=${answer.sessionId} after control reconnect")
        }
        pendingLocalIceCandidates
            .asSequence()
            .filter { it.sessionId == sessionId }
            .forEach { candidate ->
                controlSession?.sendIceCandidate(
                    candidate.sessionId,
                    candidate.candidate,
                    candidate.sdpMid,
                    candidate.sdpMlineIndex,
                )
                logInfo("Re-sent local ICE candidate session=${candidate.sessionId} after control reconnect")
            }
    }

    private fun shouldKeepActiveMediaSession(host: ControlledHostSummary): Boolean {
        return activeSession?.host?.deviceId == host.deviceId &&
            !activeSession?.sessionId.isNullOrBlank() &&
            (webRtcSession != null || pendingStopSessionId == activeSession?.sessionId)
    }

    private fun handlePreservedSessionRecoveryFailure(
        host: ControlledHostSummary,
        message: String,
        detail: String,
    ) {
        clearPendingControlReconnect()
        clearPendingSessionRecoveryValidation()
        closeControlSession()
        closeWebRtcSession()
        activeSession = null
        upsertHost(host.copy(authState = HostAuthState.PairingRequired))
        disconnectNotice = ControllerDisconnectNotice(
            title = "远程控制已断开",
            message = message,
            detail = detail,
        )
        logWarn("Preserved session recovery failed for ${host.displayName}: $detail")
    }

    private fun scheduleControlReconnect(host: ControlledHostSummary) {
        operationTimeouts.beginRecovery()
        pendingControlReconnectHost = host
        if (controlReconnectScheduled) {
            return
        }
        val nextAttempt = controlReconnectAttempt + 1
        controlReconnectAttempt = nextAttempt
        val backoffFactor = 1L shl (nextAttempt - 1).coerceAtMost(3)
        val delayMs = minOf(
            CONTROL_RECONNECT_BASE_DELAY_MS * backoffFactor,
            CONTROL_RECONNECT_MAX_DELAY_MS,
        )
        controlReconnectScheduled = true
        mainHandler.postDelayed(controlReconnectRunnable, delayMs)
        logWarn("Scheduling control reconnect for ${host.displayName} in ${delayMs}ms attempt=$nextAttempt")
    }

    private fun clearScheduledControlReconnect() {
        if (!controlReconnectScheduled) {
            return
        }
        mainHandler.removeCallbacks(controlReconnectRunnable)
        controlReconnectScheduled = false
    }

    private fun clearPendingControlReconnect() {
        clearScheduledControlReconnect()
        operationTimeouts.finishRecovery()
        pendingControlReconnectHost = null
        controlReconnectAttempt = 0
    }

    private fun schedulePendingSessionRecoveryValidation(host: ControlledHostSummary, sessionId: String) {
        pendingRecoveredSessionId = sessionId
        pendingRecoveredSessionHost = host
        mainHandler.removeCallbacks(sessionRecoveryTimeoutRunnable)
        mainHandler.postDelayed(sessionRecoveryTimeoutRunnable, SESSION_RECOVERY_TIMEOUT_MS)
    }

    private fun clearPendingSessionRecoveryValidation() {
        pendingRecoveredSessionId = null
        pendingRecoveredSessionHost = null
        mainHandler.removeCallbacks(sessionRecoveryTimeoutRunnable)
    }

    private fun confirmActiveSessionRecovery(sessionId: String) {
        if (pendingRecoveredSessionId != sessionId) return
        clearPendingSessionRecoveryValidation()
        clearPendingControlReconnect()
    }

    private fun scheduleStopSessionRetry() {
        operationTimeouts.awaitStopConfirmation()
    }

    private fun closeControlSession() {
        controlGeneration++
        controlSession?.close()
        controlSession = null
    }

    private fun isTerminalSessionState(stateLabel: String): Boolean {
        return stateLabel == "FAILED" || stateLabel == "DISCONNECTED"
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

}
