package top.akadesk.app.controller

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build

class HostDiscoveryClient(
    context: Context,
    private val onHostResolved: (ControlledHostSummary) -> Unit,
    private val onError: (String) -> Unit,
) {
    private val appContext = context.applicationContext
    private val nsdManager = appContext.getSystemService(Context.NSD_SERVICE) as NsdManager
    private var discoveryListener: NsdManager.DiscoveryListener? = null

    fun start() {
        if (discoveryListener != null) {
            return
        }
        val listener = object : NsdManager.DiscoveryListener {
            override fun onStartDiscoveryFailed(serviceType: String?, errorCode: Int) {
                onError("mDNS discovery failed to start: $errorCode")
            }

            override fun onStopDiscoveryFailed(serviceType: String?, errorCode: Int) {
                onError("mDNS discovery failed to stop: $errorCode")
            }

            override fun onDiscoveryStarted(serviceType: String?) = Unit

            override fun onDiscoveryStopped(serviceType: String?) = Unit

            override fun onServiceFound(serviceInfo: NsdServiceInfo) {
                resolve(serviceInfo)
            }

            override fun onServiceLost(serviceInfo: NsdServiceInfo) = Unit
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

    private fun resolve(serviceInfo: NsdServiceInfo) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            nsdManager.resolveService(
                serviceInfo,
                appContext.mainExecutor,
                object : NsdManager.ResolveListener {
                    override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) {
                        onError("mDNS resolve failed: $errorCode")
                    }

                    override fun onServiceResolved(resolvedServiceInfo: NsdServiceInfo) {
                        onHostResolved(resolvedServiceInfo.toControlledHostSummary())
                    }
                }
            )
        } else {
            @Suppress("DEPRECATION")
            nsdManager.resolveService(
                serviceInfo,
                object : NsdManager.ResolveListener {
                    override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) {
                        onError("mDNS resolve failed: $errorCode")
                    }

                    override fun onServiceResolved(resolvedServiceInfo: NsdServiceInfo) {
                        onHostResolved(resolvedServiceInfo.toControlledHostSummary())
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
