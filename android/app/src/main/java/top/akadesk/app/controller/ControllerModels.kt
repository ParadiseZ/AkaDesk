package top.akadesk.app.controller

data class ControlledHostSummary(
    val serviceName: String,
    val deviceId: String,
    val displayName: String,
    val platform: String,
    val hostname: String,
    val appVersion: String,
    val hostAddress: String,
    val controlPort: Int,
    val pairedAdvertised: Boolean,
    val reachable: Boolean,
    val lastSeenMs: Long,
    val authState: HostAuthState = HostAuthState.Unknown,
)

enum class HostAuthState {
    Unknown,
    PairingRequired,
    PairingPending,
    Authenticated,
    Failed,
}
