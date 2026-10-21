@file:OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
package top.akadesk.app.ui.home

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import top.akadesk.app.controller.ControllerCoordinator

@Composable
fun DisplaySettingsSheet(coordinator: ControllerCoordinator, onDismiss: () -> Unit) {
    DisposableEffect(coordinator) {
        coordinator.editingLocalInput = true
        coordinator.releaseRemoteInput()
        coordinator.refreshDisplaySettings()
        onDispose {
            coordinator.editingLocalInput = false
            coordinator.releaseRemoteInput()
        }
    }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        DisplaySettingsContent(coordinator)
    }
}

@Composable
fun DisplaySettingsContent(coordinator: ControllerCoordinator, scrollable:Boolean=true) {
        Column(Modifier.padding(16.dp).then(if(scrollable) Modifier.verticalScroll(rememberScrollState()) else Modifier), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Text("显示器设置", style = MaterialTheme.typography.titleLarge)
                TextButton(onClick = coordinator::refreshDisplaySettings, enabled = !coordinator.displaySettingsPending) { Text("刷新") }
            }
            Text("设置应用到被控电脑的当前显示器，断开连接后恢复连接前的设置。切换时画面可能短暂闪烁。")
            if (coordinator.displaySettingsPending) LinearProgressIndicator(Modifier.fillMaxWidth())
            coordinator.displaySettings?.let { snapshot ->
                val current = snapshot.currentMode
                var selection by remember(snapshot) { mutableStateOf(current) }
                val modes = snapshot.supportedModesList
                Text(snapshot.display.name.ifBlank { "主显示器" })
                Text("当前：${current.width} × ${current.height} · ${current.refreshRate} Hz · ${current.scalePercent}%")
                val resolutions = modes.map { it.width to it.height }.distinct()
                DisplayChoice("分辨率", "${selection.width} × ${selection.height}",
                    resolutions.map { "${it.first} × ${it.second}" }, !coordinator.displaySettingsPending) { index ->
                    val (width, height) = resolutions[index]
                    val candidates = modes.filter { it.width == width && it.height == height }
                    val refresh = candidates.firstOrNull { it.refreshRate == selection.refreshRate }?.refreshRate ?: candidates.first().refreshRate
                    selection = selection.toBuilder().setWidth(width).setHeight(height).setRefreshRate(refresh).build()
                }
                val rates = modes.filter { it.width == selection.width && it.height == selection.height }.map { it.refreshRate }.distinct()
                DisplayChoice("刷新率", "${selection.refreshRate} Hz", rates.map { "$it Hz" }, !coordinator.displaySettingsPending) { index ->
                    selection = selection.toBuilder().setRefreshRate(rates[index]).build()
                }
                val scales = snapshot.supportedScalePercentsList
                DisplayChoice("缩放倍率", "${selection.scalePercent}%", scales.map { "$it%" }, !coordinator.displaySettingsPending) { index ->
                    selection = selection.toBuilder().setScalePercent(scales[index]).build()
                }
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Button(onClick = { coordinator.applyDisplaySettings(selection) },
                        enabled = !coordinator.displaySettingsPending && selection != current) { Text("应用到显示器") }
                    TextButton(onClick = { coordinator.applyDisplaySettings(snapshot.historyDisplayMode) },
                        enabled = !coordinator.displaySettingsPending && current != snapshot.historyDisplayMode) { Text("恢复连接前设置") }
                }
            }
            coordinator.displaySettingsError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            Spacer(Modifier.height(12.dp))
        }
}

@Composable
private fun DisplayChoice(label: String, value: String, choices: List<String>, enabled: Boolean, onSelect: (Int) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        OutlinedButton(onClick = { expanded = true }, enabled = enabled && choices.isNotEmpty(), modifier = Modifier.fillMaxWidth()) {
            Text("$label：$value")
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            choices.forEachIndexed { index, name ->
                DropdownMenuItem(text = { Text(name) }, onClick = { expanded = false; onSelect(index) })
            }
        }
    }
}
