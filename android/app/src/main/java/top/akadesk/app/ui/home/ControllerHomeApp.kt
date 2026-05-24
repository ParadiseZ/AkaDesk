@file:OptIn(
    androidx.compose.material.ExperimentalMaterialApi::class,
    androidx.compose.material3.ExperimentalMaterial3Api::class,
)

package top.akadesk.app.ui.home

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.pullrefresh.pullRefresh
import androidx.compose.material.pullrefresh.PullRefreshIndicator
import androidx.compose.material.pullrefresh.rememberPullRefreshState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.ScreenShare
import androidx.compose.material.icons.outlined.Computer
import androidx.compose.material.icons.outlined.Devices
import androidx.compose.material.icons.outlined.Keyboard
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material.icons.outlined.PersonOutline
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Tv
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import top.akadesk.app.controller.ActiveRemoteSession
import top.akadesk.app.controller.ControlledHostSummary
import top.akadesk.app.controller.ControllerCoordinator
import top.akadesk.app.controller.ControllerPairRecord
import top.akadesk.app.controller.ControllerVideoCodecPreset

private enum class RootTab { Connect, Mine }
private enum class ConnectListTab(val title: String) { Found("发现"), History("历史"), Favorite("收藏"), Paired("已配对") }

@Composable
fun ControllerHomeApp(
    coordinator: ControllerCoordinator,
    modifier: Modifier = Modifier,
) {
    var rootTab by rememberSaveable { mutableStateOf(RootTab.Connect) }
    var settingsPageOpen by rememberSaveable { mutableStateOf(false) }
    var connectListTab by rememberSaveable { mutableStateOf(ConnectListTab.Found) }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(Color(0xFFF4F7FB))
            .statusBarsPadding(),
    ) {
        when {
            coordinator.activeSession != null -> RemoteStreamScreen(
                session = coordinator.activeSession!!,
                onDisconnect = coordinator::endRemoteSession,
            )

            settingsPageOpen -> CodecSettingsPage(
                selectedPreset = coordinator.preferredVideoCodec,
                onBack = { settingsPageOpen = false },
                onSelect = coordinator::updatePreferredVideoCodec,
            )

            else -> Scaffold(
                containerColor = Color.Transparent,
                contentWindowInsets = WindowInsets.safeDrawing,
                bottomBar = {
                    NavigationBar(
                        containerColor = Color.White,
                        tonalElevation = 0.dp,
                    ) {
                        NavigationBarItem(
                            selected = rootTab == RootTab.Connect,
                            onClick = { rootTab = RootTab.Connect },
                            icon = { Icon(Icons.Outlined.Devices, contentDescription = null) },
                            label = { Text("连接") },
                        )
                        NavigationBarItem(
                            selected = rootTab == RootTab.Mine,
                            onClick = { rootTab = RootTab.Mine },
                            icon = { Icon(Icons.Outlined.PersonOutline, contentDescription = null) },
                            label = { Text("我的") },
                        )
                    }
                },
            ) { innerPadding ->
                when (rootTab) {
                    RootTab.Connect -> ConnectPage(
                        coordinator = coordinator,
                        selectedTab = connectListTab,
                        onTabSelected = { connectListTab = it },
                        modifier = Modifier.padding(innerPadding),
                    )

                    RootTab.Mine -> MinePage(
                        onOpenCodecSettings = { settingsPageOpen = true },
                        modifier = Modifier.padding(innerPadding),
                    )
                }
            }
        }

        coordinator.selectedHostForActions?.let { host ->
            HostActionSheet(
                host = host,
                hasTrustedPair = coordinator.pairedDevices.any { it.deviceId == host.deviceId },
                onDismiss = coordinator::dismissSheets,
                onTrustedConnect = { coordinator.connectRememberedHost(host) },
                onVerification = { coordinator.openVerificationSheet(host) },
                onAwaitApproval = { coordinator.openWaitingApprovalSheet(host) },
            )
        }

        coordinator.verificationTarget?.let { host ->
            VerificationCodeSheet(
                host = host,
                verificationCode = coordinator.verificationCode,
                onVerificationChange = { coordinator.verificationCode = it },
                onDismiss = coordinator::dismissSheets,
                onConfirm = coordinator::submitVerificationConnect,
            )
        }

        coordinator.waitingApprovalTarget?.let { host ->
            WaitingApprovalSheet(
                host = host,
                startedAtMs = coordinator.waitingApprovalStartedAtMs,
                onDismiss = coordinator::cancelWaitingApprovalSheet,
            )
        }

        coordinator.disconnectNotice?.let { notice ->
            androidx.compose.ui.window.Dialog(onDismissRequest = coordinator::dismissDisconnectNotice) {
                Surface(
                    shape = RoundedCornerShape(24.dp),
                    color = Color(0xFF0A1020),
                ) {
                    Column(
                        modifier = Modifier
                            .width(280.dp)
                            .padding(horizontal = 24.dp, vertical = 22.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Box(
                            modifier = Modifier
                                .size(70.dp)
                                .clip(CircleShape)
                                .background(Color(0xFFFF5A64)),
                            contentAlignment = Alignment.Center,
                        ) {
                            Text("×", color = Color.White, style = MaterialTheme.typography.displaySmall)
                        }
                        Text(notice.title, color = Color.White, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                        Text(notice.message, color = Color(0xFFDCE8FF), style = MaterialTheme.typography.bodyLarge)
                        if (notice.detail.isNotBlank()) {
                            Text(notice.detail, color = Color(0xFF9BB0D0), style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
                        }
                        TextButton(onClick = coordinator::dismissDisconnectNotice) { Text("知道了") }
                    }
                }
            }
        }
    }
}

@Composable
private fun ConnectPage(
    coordinator: ControllerCoordinator,
    selectedTab: ConnectListTab,
    onTabSelected: (ConnectListTab) -> Unit,
    modifier: Modifier = Modifier,
) {
    val records = when (selectedTab) {
        ConnectListTab.Found -> coordinator.hosts
        ConnectListTab.History -> coordinator.historyDevices.map { it.toHostSummary(coordinator) }
        ConnectListTab.Favorite -> coordinator.favoriteDevices.map { it.toHostSummary(coordinator) }
        ConnectListTab.Paired -> coordinator.pairedDevices.map { it.toHostSummary(coordinator) }
    }
    val scope = rememberCoroutineScope()
    var refreshing by remember(selectedTab) { mutableStateOf(false) }
    val pullRefreshState = rememberPullRefreshState(
        refreshing = refreshing,
        onRefresh = {
            if (selectedTab == ConnectListTab.Found) {
                refreshing = true
                coordinator.startDiscovery()
                scope.launch {
                    delay(900L)
                    refreshing = false
                }
            }
        },
    )

    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("连接", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
        Surface(
            shape = RoundedCornerShape(18.dp),
            color = Color.White,
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                OutlinedTextField(
                    value = coordinator.remoteAssistDeviceId,
                    onValueChange = { coordinator.remoteAssistDeviceId = it },
                    modifier = Modifier.weight(1f),
                    placeholder = { Text("远程ID") },
                    singleLine = true,
                    shape = RoundedCornerShape(14.dp),
                )
                Button(
                    onClick = coordinator::beginRemoteAssistFromInput,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = Color(0xFF59B7FF),
                        contentColor = Color.White,
                    ),
                    shape = RoundedCornerShape(12.dp),
                ) {
                    Text("连接")
                }
            }
        }
        CompactTopTabs(selectedTab = selectedTab, onTabSelected = onTabSelected)
        Box(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth(),
        ) {
            LazyColumn(
                modifier = Modifier
                    .fillMaxSize()
                    .then(if (selectedTab == ConnectListTab.Found) Modifier.pullRefresh(pullRefreshState) else Modifier),
                contentPadding = PaddingValues(bottom = 14.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                if (records.isEmpty()) {
                    item {
                        Surface(shape = RoundedCornerShape(18.dp), color = Color.White) {
                            Box(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(vertical = 26.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(
                                    if (selectedTab == ConnectListTab.Found) "下拉发现设备" else "暂无设备",
                                    color = Color(0xFF8894A8),
                                )
                            }
                        }
                    }
                } else {
                    items(records, key = { it.deviceId.ifBlank { it.serviceName } }) { host ->
                        CompactDeviceRow(
                            host = host,
                            tab = selectedTab,
                            isFavorite = coordinator.favoriteDevices.any { it.deviceId == host.deviceId },
                            onConnect = {
                                if (coordinator.pairedDevices.any { it.deviceId == host.deviceId }) {
                                    coordinator.connectRememberedHost(host)
                                } else {
                                    coordinator.openHostActions(host)
                                }
                            },
                            onDelete = {
                                when (selectedTab) {
                                    ConnectListTab.Found -> coordinator.removeFoundHost(host.deviceId)
                                    ConnectListTab.History -> coordinator.removeHistory(host.deviceId)
                                    ConnectListTab.Favorite -> coordinator.removeFavorite(host.deviceId)
                                    ConnectListTab.Paired -> coordinator.removePairedDevice(
                                        coordinator.pairedDevices.first { it.deviceId == host.deviceId },
                                    )
                                }
                            },
                            onFavoriteToggle = {
                                if (coordinator.favoriteDevices.any { it.deviceId == host.deviceId }) {
                                    coordinator.removeFavorite(host.deviceId)
                                } else {
                                    coordinator.addFavorite(host)
                                }
                            },
                        )
                    }
                }
            }
            if (selectedTab == ConnectListTab.Found) {
                PullRefreshIndicator(
                    refreshing = refreshing,
                    state = pullRefreshState,
                    modifier = Modifier.align(Alignment.TopCenter),
                    backgroundColor = Color.White,
                    contentColor = Color(0xFF59B7FF),
                )
            }
        }
    }
}

@Composable
private fun MinePage(
    onOpenCodecSettings: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("我的", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
        Surface(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(18.dp),
            color = Color.White,
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(onClick = onOpenCodecSettings)
                    .padding(horizontal = 14.dp, vertical = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                Icon(Icons.Outlined.Settings, contentDescription = null, tint = Color(0xFF2C74EE))
                Text("编码设置", modifier = Modifier.weight(1f), fontWeight = FontWeight.SemiBold)
                Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = null, tint = Color.Transparent)
                Text(">", color = Color(0xFF9AA6BA))
            }
        }
    }
}

@Composable
private fun CodecSettingsPage(
    selectedPreset: ControllerVideoCodecPreset,
    onBack: () -> Unit,
    onSelect: (ControllerVideoCodecPreset) -> Unit,
) {
    Scaffold(
        containerColor = Color(0xFFF4F7FB),
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = {
            Surface(color = Color.White) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 8.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = null)
                    }
                    Text("编码设置", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                }
            }
        },
    ) { innerPadding ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding),
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 14.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            items(ControllerVideoCodecPreset.entries, key = { it.storageKey }) { preset ->
                Surface(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(18.dp),
                    color = Color.White,
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { onSelect(preset) }
                            .padding(horizontal = 14.dp, vertical = 16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Icon(Icons.Outlined.Tv, contentDescription = null, tint = Color(0xFF2C74EE))
                        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(preset.title, fontWeight = FontWeight.SemiBold)
                        }
                        if (selectedPreset == preset) {
                            Text("当前", color = Color(0xFF2C74EE), fontWeight = FontWeight.SemiBold)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun CompactDeviceRow(
    host: ControlledHostSummary,
    tab: ConnectListTab,
    isFavorite: Boolean,
    onConnect: () -> Unit,
    onDelete: () -> Unit,
    onFavoriteToggle: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(18.dp),
        color = Color.White,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 14.dp, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            SystemIcon(platform = host.platform)
            Column(
                modifier = Modifier
                    .weight(1f)
                    .clickable(onClick = onConnect),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                Text(host.deviceId.ifBlank { "--" }, fontWeight = FontWeight.SemiBold)
                Text(
                    host.displayName.ifBlank { "未命名设备" },
                    color = Color(0xFF7A8699),
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            Box {
                IconButton(onClick = { expanded = true }) {
                    Icon(Icons.Outlined.MoreVert, contentDescription = null, tint = Color(0xFF5E6980))
                }
                DropdownMenu(
                    expanded = expanded,
                    onDismissRequest = { expanded = false },
                ) {
                    DropdownMenuItem(
                        text = { Text("连接") },
                        onClick = {
                            expanded = false
                            onConnect()
                        },
                    )
                    if (tab == ConnectListTab.Found) {
                        DropdownMenuItem(
                            text = { Text(if (isFavorite) "取消收藏" else "收藏") },
                            onClick = {
                                expanded = false
                                onFavoriteToggle()
                            },
                        )
                    }
                    DropdownMenuItem(
                        text = {
                            Text(
                                when (tab) {
                                    ConnectListTab.Paired -> "取消配对"
                                    ConnectListTab.Favorite -> "取消收藏"
                                    else -> "删除"
                                },
                            )
                        },
                        onClick = {
                            expanded = false
                            onDelete()
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun CompactTopTabs(
    selectedTab: ConnectListTab,
    onTabSelected: (ConnectListTab) -> Unit,
) {
    val density = LocalDensity.current
    Surface(
        color = Color.White,
        shape = RoundedCornerShape(16.dp),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 10.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ConnectListTab.entries.forEach { tab ->
                val selected = tab == selectedTab
                var textWidthPx by remember(tab) { mutableStateOf(0) }
                val targetUnderlineWidth = with(density) { textWidthPx.toDp() }
                val underlineWidth by animateDpAsState(
                    if (selected) targetUnderlineWidth else 0.dp,
                    label = "tabUnderlineWidth",
                )
                val textScale by animateFloatAsState(if (selected) 1f else 0.98f, label = "tabTextScale")
                Column(
                    modifier = Modifier
                        .clickable { onTabSelected(tab) }
                        .padding(horizontal = 2.dp, vertical = 8.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(
                        text = tab.title,
                        color = Color.Black,
                        fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                        modifier = Modifier
                            .scale(textScale)
                            .onSizeChanged { textWidthPx = it.width },
                    )
                    Box(
                        modifier = Modifier
                            .width(underlineWidth)
                            .height(2.dp)
                            .background(Color(0xFF59B7FF)),
                    )
                }
            }
        }
    }
}

@Composable
private fun HostActionSheet(
    host: ControlledHostSummary,
    hasTrustedPair: Boolean,
    onDismiss: () -> Unit,
    onTrustedConnect: () -> Unit,
    onVerification: () -> Unit,
    onAwaitApproval: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = Color.White,
        shape = RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp, vertical = 8.dp),
        ) {
            Text(host.displayName.ifBlank { host.deviceId }, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
            Text(host.deviceId, color = Color(0xFF7A8699), modifier = Modifier.padding(top = 4.dp, bottom = 14.dp))
            if (hasTrustedPair) {
                SheetActionRow(label = "记住后免密连接", onClick = onTrustedConnect)
                HorizontalDivider()
            }
            SheetActionRow(label = "输入验证码连接", onClick = onVerification)
            HorizontalDivider()
            SheetActionRow(label = "等待对方允许", onClick = onAwaitApproval)
            Spacer(modifier = Modifier.height(12.dp))
        }
    }
}

@Composable
private fun VerificationCodeSheet(
    host: ControlledHostSummary,
    verificationCode: String,
    onVerificationChange: (String) -> Unit,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = Color.White,
        shape = RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("输入设备验证码", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                TextButton(onClick = onDismiss) { Text("取消") }
            }
            Text(host.deviceId, color = Color(0xFF7A8699))
            OutlinedTextField(
                value = verificationCode,
                onValueChange = onVerificationChange,
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("请输入设备验证码") },
                singleLine = true,
                shape = RoundedCornerShape(14.dp),
            )
            OutlinedButton(onClick = onConfirm, modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
                Text("开始远控")
            }
            Spacer(modifier = Modifier.height(8.dp))
        }
    }
}

@Composable
private fun WaitingApprovalSheet(
    host: ControlledHostSummary,
    startedAtMs: Long,
    onDismiss: () -> Unit,
) {
    val now = rememberTickerMillis()
    val seconds = (now - startedAtMs) / 1_000L
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = Color.White,
        shape = RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp, vertical = 14.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text("发起远程协助", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
            CircularProgressIndicator(strokeWidth = 3.dp)
            Text("${host.displayName} 等待允许 (${seconds}s)", style = MaterialTheme.typography.bodyLarge)
            TextButton(onClick = onDismiss) { Text("取消") }
        }
    }
}

@Composable
private fun RemoteStreamScreen(
    session: ActiveRemoteSession,
    onDisconnect: () -> Unit,
) {
    val now = rememberTickerMillis()
    Row(
        modifier = Modifier
            .fillMaxSize()
            .background(Color(0xFF04070C))
            .navigationBarsPadding(),
    ) {
        Box(
            modifier = Modifier
                .weight(1f)
                .fillMaxHeight()
                .padding(14.dp)
                .clip(RoundedCornerShape(20.dp))
                .background(
                    Brush.linearGradient(
                        listOf(Color(0xFF7FB0FF), Color(0xFF2D58CF), Color(0xFF0A1020)),
                    ),
                ),
        ) {
            Column(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .padding(20.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(session.host.displayName.ifBlank { "Windows" }, color = Color.White, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                Text(formatDuration(now - session.startedAtMs), color = Color(0xFFD7E3FF))
            }
            Column(
                modifier = Modifier.align(Alignment.Center),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                if (session.stateLabel != "STREAMING") {
                    CircularProgressIndicator(color = Color.White, strokeWidth = 4.dp)
                }
                Text(
                    if (session.stateLabel == "STREAMING") "显示桌面" else "启动画面传输",
                    color = Color.White,
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.Bold,
                )
            }
        }
        Surface(
            modifier = Modifier
                .width(92.dp)
                .fillMaxHeight(),
            color = Color.Black,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxHeight()
                    .padding(vertical = 18.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                StreamAction(Icons.Outlined.Settings, "设置")
                StreamAction(Icons.Outlined.Keyboard, "键盘")
                StreamAction(Icons.Outlined.Tv, "显示桌面")
                Spacer(modifier = Modifier.weight(1f))
                OutlinedButton(
                    onClick = onDisconnect,
                    modifier = Modifier
                        .padding(horizontal = 10.dp)
                        .fillMaxWidth(),
                    shape = RoundedCornerShape(14.dp),
                ) {
                    Text("断开")
                }
            }
        }
    }
}

@Composable
private fun SheetActionRow(
    label: String,
    onClick: () -> Unit,
) {
    TextButton(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Text(label, modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Start)
    }
}

@Composable
private fun StreamAction(
    icon: ImageVector,
    label: String,
) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            modifier = Modifier
                .size(52.dp)
                .clip(RoundedCornerShape(16.dp))
                .background(Color(0xFF11161E)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = null, tint = Color.White)
        }
        Text(label, color = Color.White, style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
    }
}

@Composable
private fun SystemIcon(platform: String) {
    val icon = when {
        platform.contains("windows", ignoreCase = true) -> Icons.Outlined.Computer
        platform.contains("android", ignoreCase = true) -> Icons.Outlined.Devices
        else -> Icons.Outlined.Computer
    }
    Box(
        modifier = Modifier
            .size(38.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(Color(0xFFF0F4FA)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, tint = Color(0xFF356FE0))
    }
}

private fun ControllerPairRecord.toHostSummary(coordinator: ControllerCoordinator): ControlledHostSummary {
    return ControlledHostSummary(
        serviceName = displayName.ifBlank { deviceId },
        deviceId = deviceId,
        displayName = displayName,
        platform = platform,
        hostname = "",
        appVersion = "",
        hostAddress = hostAddress,
        controlPort = 46661,
        pairedAdvertised = coordinator.pairedDevices.any { it.deviceId == deviceId },
        reachable = false,
        lastSeenMs = savedAtMs,
    )
}

@Composable
private fun rememberTickerMillis(): Long {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(1000L)
            now = System.currentTimeMillis()
        }
    }
    return now
}

private fun formatDuration(durationMs: Long): String {
    val totalSeconds = (durationMs / 1000L).coerceAtLeast(0L)
    val minutes = totalSeconds / 60
    val seconds = totalSeconds % 60
    return "%02d:%02d".format(minutes, seconds)
}
