package top.akadesk.app.controller

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build

class HostDiscoveryClient(
    context: Context,
    private val onHostResolved: (ControlledHostSummary) -> Unit,
    private val onError: (String) -> Unit,
    private val onHostLost: (String) -> Unit,
) {
    private val appContext = context.applicationContext
    private val nsdManager = appContext.getSystemService(Context.NSD_SERVICE) as NsdManager
    private var discoveryListener: NsdManager.DiscoveryListener? = null

    fun start() {
        stop()
        val listener = object : NsdManager.DiscoveryListener {
            override fun onStartDiscoveryFailed(serviceType: String?, errorCode: Int) {
                if (discoveryListener !== this) return
                discoveryListener = null
                onError("mDNS discovery failed to start: $errorCode")
            }

            override fun onStopDiscoveryFailed(serviceType: String?, errorCode: Int) {
                if (discoveryListener === this) onError("mDNS discovery failed to stop: $errorCode")
            }

            override fun onDiscoveryStarted(serviceType: String?) = Unit

            override fun onDiscoveryStopped(serviceType: String?) = Unit

            override fun onServiceFound(serviceInfo: NsdServiceInfo) {
                if (discoveryListener === this) resolve(serviceInfo, this)
            }

            override fun onServiceLost(serviceInfo: NsdServiceInfo) {
                if (discoveryListener === this) onHostLost(serviceInfo.serviceName)
            }
        }
        discoveryListener = listener
        nsdManager.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, listener)
    }

    fun stop() {
        discoveryListener?.let { listener ->
            runCatching { nsdManager.stopServiceDiscovery(listener) }
        }
        discoveryListener = null
    }

    private fun resolve(serviceInfo: NsdServiceInfo, owner: NsdManager.DiscoveryListener) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            nsdManager.resolveService(
                serviceInfo,
                appContext.mainExecutor,
                object : NsdManager.ResolveListener {
                    override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) {
                        if (discoveryListener === owner) onError("mDNS resolve failed: $errorCode")
                    }

                    override fun onServiceResolved(resolvedServiceInfo: NsdServiceInfo) {
                        if (discoveryListener === owner) onHostResolved(resolvedServiceInfo.toControlledHostSummary())
                    }
                }
            )
        } else {
            @Suppress("DEPRECATION")
            nsdManager.resolveService(
                serviceInfo,
                object : NsdManager.ResolveListener {
                    override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) {
                        if (discoveryListener === owner) onError("mDNS resolve failed: $errorCode")
                    }

                    override fun onServiceResolved(resolvedServiceInfo: NsdServiceInfo) {
                        if (discoveryListener === owner) onHostResolved(resolvedServiceInfo.toControlledHostSummary())
                    }
                }
            )
        }
    }

    private fun NsdServiceInfo.toControlledHostSummary(): ControlledHostSummary {
        val attrs = attributes.mapValues { (_, value) -> value.decodeToString() }
        return ControlledHostSummary(
            serviceName = serviceName,
            deviceId = attrs["device_id"].orEmpty(),
            displayName = attrs["display_name"].orEmpty().ifBlank { serviceName },
            platform = attrs["platform"].orEmpty(),
            hostname = attrs["hostname"].orEmpty(),
            appVersion = attrs["app_version"].orEmpty(),
            hostAddress = host?.hostAddress.orEmpty(),
            controlPort = port,
            pairedAdvertised = attrs["paired"]?.toBooleanStrictOrNull() ?: false,
            reachable = attrs["reachable"]?.toBooleanStrictOrNull() ?: true,
            lastSeenMs = attrs["last_seen_ms"]?.toLongOrNull() ?: 0L,
        )
    }

    companion object {
        private const val SERVICE_TYPE = "_akadesk._tcp."
    }
}
