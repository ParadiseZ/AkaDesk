package top.akadesk.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import top.akadesk.app.controller.ControlledHostSummary
import top.akadesk.app.controller.ControllerCoordinator
import top.akadesk.app.ui.theme.AkaDeskTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            AkaDeskTheme {
                val coordinator = remember { ControllerCoordinator(this) }
                DisposableEffect(Unit) {
                    onDispose { coordinator.stopDiscovery() }
                }
                Scaffold(modifier = Modifier.fillMaxSize()) { innerPadding ->
                    ControllerHomeScreen(
                        coordinator = coordinator,
                        modifier = Modifier.padding(innerPadding),
                    )
                }
            }
        }
    }
}

@Composable
fun ControllerHomeScreen(
    coordinator: ControllerCoordinator,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("AkaDesk M4 Controller")
        Text(coordinator.statusMessage)
        Button(onClick = coordinator::startDiscovery) {
            Text("Scan via mDNS")
        }
        if (coordinator.authenticatedTarget != null) {
            Button(onClick = coordinator::preparePreviewSession) {
                Text("Prepare H.264 preview")
            }
        }
        coordinator.activeSessionId?.let { sessionId ->
            Text("Session: $sessionId")
        }
        coordinator.negotiatedVideoSummary?.let { summary ->
            Text("Video: $summary")
        }
        LazyColumn(
            modifier = Modifier.weight(1f),
            contentPadding = PaddingValues(vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            items(coordinator.hosts, key = { it.deviceId.ifBlank { it.serviceName } }) { host ->
                HostCard(host = host, onConnect = { coordinator.connect(host) })
            }
        }
        if (coordinator.pairingTarget != null) {
            OutlinedTextField(
                value = coordinator.pairingCode,
                onValueChange = { coordinator.pairingCode = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Host verification code") },
            )
            Button(onClick = coordinator::startPairing) {
                Text("Request pairing challenge")
            }
            Button(onClick = coordinator::submitPairingCode) {
                Text("Submit code and reconnect")
            }
        }
    }
}

@Composable
fun HostCard(
    host: ControlledHostSummary,
    onConnect: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(host.displayName)
            Text("${host.hostAddress}:${host.controlPort} • ${host.platform.ifBlank { "unknown" }}")
            Text("Auth: ${host.authState.name}")
            Text("Advertised paired: ${host.pairedAdvertised} • reachable: ${host.reachable}")
            Button(onClick = onConnect, modifier = Modifier.align(Alignment.End)) {
                Text("Connect")
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
fun ControllerPreview() {
    AkaDeskTheme {
        HostCard(
            host = ControlledHostSummary(
                serviceName = "AkaDesk Preview",
                deviceId = "preview",
                displayName = "Preview Host",
                platform = "windows",
                hostname = "preview-host",
                appVersion = "0.1.0",
                hostAddress = "192.168.0.8",
                controlPort = 46661,
                pairedAdvertised = false,
                reachable = true,
                lastSeenMs = 0L,
            ),
            onConnect = {},
        )
    }
}
