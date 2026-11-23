package com.opencloudgaming.opennow

import android.Manifest
import android.content.Context
import android.content.res.Configuration
import android.content.Intent
import android.hardware.input.InputManager
import android.net.Uri
import android.view.InputDevice
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.math.min
import kotlin.math.floor

internal enum class LoginService { Nvidia, Boosteroid }

@Composable
internal fun LoginServiceSelector(
    selected: LoginService?,
    onNvidia: () -> Unit,
    onBoosteroid: () -> Unit,
    nvidiaChoice: (@Composable () -> Unit)? = null,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        if (nvidiaChoice != null) nvidiaChoice()
        else FilterChip(
            selected = selected == LoginService.Nvidia,
            onClick = onNvidia,
            label = { Text(stringResource(R.string.login_service_nvidia)) },
        )
        FilterChip(
            selected = selected == LoginService.Boosteroid,
            onClick = onBoosteroid,
            label = { Text(stringResource(R.string.boosteroid_name)) },
        )
    }
}

@Composable
internal fun LoginScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val signInFocusRequester = remember { FocusRequester() }
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var tokenDialogVisible by remember { mutableStateOf(false) }
    var tokenInput by remember { mutableStateOf("") }
    var pendingLogText by remember { mutableStateOf("") }
    var logExportInProgress by remember { mutableStateOf(false) }
    val logExportLauncher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/plain")) { uri ->
        if (uri == null) {
            logExportInProgress = false
            return@rememberLauncherForActivityResult
        }
        val logText = pendingLogText
        scope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    context.contentResolver.openOutputStream(uri)?.use { output ->
                        output.write(logText.toByteArray(Charsets.UTF_8))
                    } ?: error("Could not open log file")
                }
            }
            result.onSuccess {
                Toast.makeText(context, context.getString(R.string.login_logs_exported), Toast.LENGTH_SHORT).show()
            }.onFailure { error ->
                Toast.makeText(
                    context,
                    error.message ?: context.getString(R.string.login_logs_export_failed),
                    Toast.LENGTH_LONG,
                ).show()
            }
            logExportInProgress = false
        }
    }
    val tvLogin = state.androidTvProfile
    val deviceCodeLoginAvailable = state.selectedProvider.supportsDeviceCodeLogin
    val preferDeviceCodeLogin = tvLogin && deviceCodeLoginAvailable
    val deviceLoginPrompt = state.deviceLoginPrompt.takeIf { deviceCodeLoginAvailable }
    val normalLoginBusy = state.launchPhase.isNotBlank() && deviceLoginPrompt == null
    LaunchedEffect(preferDeviceCodeLogin, deviceLoginPrompt == null) {
        if (preferDeviceCodeLogin && deviceLoginPrompt == null) {
            runCatching { signInFocusRequester.requestFocus() }
        }
    }
    if (preferDeviceCodeLogin && deviceLoginPrompt != null) {
        TvDeviceLoginScreen(
            prompt = deviceLoginPrompt,
            phase = state.launchPhase,
            onCancel = viewModel::cancelLogin,
        )
        return
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val compactForPhonePairing = tvLogin && state.localTvConnector.hosting
        val dedicatedPhonePairing = shouldUseDedicatedTvPairingLayout(
            tvProfile = tvLogin,
            hosting = state.localTvConnector.hosting,
            availableWidthDp = maxWidth.value,
            availableHeightDp = maxHeight.value,
        )
        if (dedicatedPhonePairing) {
            TvPhonePairingPanel(
                state = state,
                viewModel = viewModel,
                dedicated = true,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(if (compactForPhonePairing) 12.dp else 24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                OpenNowMark(
                    size = if (compactForPhonePairing) 56.dp else 88.dp,
                    modifier = Modifier.clickable(onClick = viewModel::recordLoginIconTap),
                )
                Spacer(Modifier.height(if (compactForPhonePairing) 8.dp else 20.dp))
                Text(
                    stringResource(R.string.app_name),
                    color = TextPrimary,
                    style = if (compactForPhonePairing) MaterialTheme.typography.headlineLarge else MaterialTheme.typography.displaySmall,
                    fontWeight = FontWeight.Bold,
                )
                Spacer(Modifier.height(if (compactForPhonePairing) 12.dp else 24.dp))
                if (BOOSTEROID_AVAILABLE) {
                    if (deviceLoginPrompt == null && !normalLoginBusy) {
                        val showAllianceOptions = state.providers.size > 1 ||
                            !state.selectedProvider.code.equals("NVIDIA", ignoreCase = true)
                        LoginServiceSelector(
                            selected = LoginService.Nvidia,
                            onNvidia = {},
                            onBoosteroid = { openBoosteroid(context) },
                            nvidiaChoice = if (showAllianceOptions) {
                                {
                                    ProviderPicker(
                                        state.providers,
                                        state.selectedProvider,
                                        viewModel::selectProvider,
                                        serviceName = stringResource(R.string.login_service_nvidia),
                                    )
                                }
                            } else null,
                        )
                    }
                } else {
                    ProviderPicker(state.providers, state.selectedProvider, viewModel::selectProvider)
                }
                Spacer(Modifier.height(if (compactForPhonePairing) 8.dp else 16.dp))
                deviceLoginPrompt?.let { prompt ->
                    DeviceLoginPanel(prompt = prompt, phase = state.launchPhase, onCancel = viewModel::cancelLogin)
                } ?: Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Button(
                        onClick = { viewModel.login() },
                        enabled = !normalLoginBusy,
                        modifier = Modifier.focusRequester(signInFocusRequester),
                        colors = ButtonDefaults.buttonColors(
                            disabledContainerColor = MaterialTheme.colorScheme.primary.copy(alpha = 0.72f),
                            disabledContentColor = MaterialTheme.colorScheme.onPrimary,
                        ),
                    ) {
                        if (normalLoginBusy) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(18.dp),
                                strokeWidth = 2.dp,
                                color = MaterialTheme.colorScheme.onPrimary,
                            )
                            Spacer(Modifier.width(10.dp))
                        }
                        Text(
                            when {
                                state.launchPhase.isNotBlank() -> state.launchPhase
                                BOOSTEROID_AVAILABLE -> stringResource(R.string.action_sign_in)
                                preferDeviceCodeLogin -> stringResource(R.string.login_tv_start, state.selectedProvider.displayName)
                                else -> stringResource(R.string.login_with_provider, state.selectedProvider.displayName)
                            },
                        )
                    }
                    if (!tvLogin && deviceCodeLoginAvailable) {
                        TextButton(onClick = { viewModel.loginWithCode() }, enabled = !normalLoginBusy) {
                            Text(stringResource(R.string.login_use_code))
                        }
                    }
                    if (tvLogin) {
                        TvPhonePairingPanel(state = state, viewModel = viewModel)
                    }
                }
                if (state.error != null) {
                    Spacer(Modifier.height(14.dp))
                    Text(state.error.orEmpty(), color = Color(0xffff9f9f))
                }
            }
        }
    }

    if (state.loginToolsVisible) {
        AlertDialog(
            onDismissRequest = viewModel::dismissLoginTools,
            title = { Text(stringResource(R.string.login_tools_title)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(stringResource(R.string.login_tools_body))
                    Button(
                        onClick = {
                            viewModel.dismissLoginTools()
                            tokenDialogVisible = true
                        },
                        enabled = !normalLoginBusy,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(stringResource(R.string.login_token_title))
                    }
                    OutlinedButton(
                        onClick = {
                            viewModel.dismissLoginTools()
                            if (tvLogin) {
                                viewModel.requestDiagnosticShare()
                            } else {
                                logExportInProgress = true
                                scope.launch {
                                    runCatching { viewModel.sanitizedDebugLogText() }
                                        .onSuccess { logs ->
                                            pendingLogText = logs
                                            logExportLauncher.launch(viewModel.debugLogFileName())
                                        }
                                        .onFailure { error ->
                                            Toast.makeText(
                                                context,
                                                error.message ?: context.getString(R.string.login_logs_export_failed),
                                                Toast.LENGTH_LONG,
                                            ).show()
                                            logExportInProgress = false
                                        }
                                }
                            }
                        },
                        enabled = !logExportInProgress,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(stringResource(if (tvLogin) R.string.login_export_logs_qr else R.string.settings_export_logs))
                    }
                }
            },
            confirmButton = {},
            dismissButton = {
                TextButton(onClick = viewModel::dismissLoginTools) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }

    if (tokenDialogVisible) {
        val submitToken = {
            val submittedToken = tokenInput
            tokenInput = ""
            tokenDialogVisible = false
            viewModel.loginWithToken(submittedToken)
        }
        AlertDialog(
            onDismissRequest = {
                tokenInput = ""
                tokenDialogVisible = false
            },
            title = { Text(stringResource(R.string.login_token_title)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Text(stringResource(R.string.login_token_body))
                    OutlinedTextField(
                        value = tokenInput,
                        onValueChange = { tokenInput = it },
                        modifier = Modifier.fillMaxWidth(),
                        label = { Text(stringResource(R.string.login_access_token)) },
                        minLines = 3,
                        maxLines = 6,
                        visualTransformation = PasswordVisualTransformation(),
                        keyboardOptions = KeyboardOptions(
                            keyboardType = KeyboardType.Password,
                            imeAction = ImeAction.Done,
                        ),
                        keyboardActions = KeyboardActions(
                            onDone = { if (tokenInput.isNotBlank() && !normalLoginBusy) submitToken() },
                        ),
                        singleLine = false,
                    )
                    Text(
                        stringResource(R.string.login_token_warning),
                        color = TextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            },
            confirmButton = {
                Button(
                    onClick = submitToken,
                    enabled = tokenInput.isNotBlank() && !normalLoginBusy,
                ) {
                    Text(stringResource(R.string.action_sign_in))
                }
            },
            dismissButton = {
                TextButton(
                    onClick = {
                        tokenInput = ""
                        tokenDialogVisible = false
                    },
                ) {
                    Text(stringResource(R.string.action_cancel))
                }
            },
        )
    }
}

internal fun shouldUseDedicatedTvPairingLayout(
    tvProfile: Boolean,
    hosting: Boolean,
    availableWidthDp: Float,
    availableHeightDp: Float,
): Boolean = tvProfile && hosting && (availableHeightDp < 500f || availableWidthDp < 760f)

@Composable
internal fun TvPhonePairingPanel(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    dedicated: Boolean = false,
    modifier: Modifier = Modifier,
) {
    val connector = state.localTvConnector
    val context = LocalContext.current
    val localNetworkPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (granted) viewModel.startLocalTvConnector()
    }
    if (!connector.hosting) {
        OutlinedButton(
            onClick = {
                if (context.hasAndroidLocalNetworkAccess()) {
                    viewModel.startLocalTvConnector()
                } else {
                    localNetworkPermissionLauncher.launch(Manifest.permission.ACCESS_LOCAL_NETWORK)
                }
            },
            enabled = !connector.busy,
            modifier = modifier,
        ) {
            Text(
                if (connector.busy) {
                    stringResource(R.string.tv_pair_starting)
                } else {
                    stringResource(R.string.tv_pair_start)
                },
            )
        }
    } else {
        val qrCode = remember(connector.pairUri) { connector.pairUri?.let(QrCode::encodeText) }
        Card(
            colors = CardDefaults.cardColors(containerColor = PanelAlt),
            shape = RoundedCornerShape(if (dedicated) 26.dp else 18.dp),
            modifier = if (dedicated) {
                modifier.padding(12.dp)
            } else {
                modifier.fillMaxWidth().padding(top = 8.dp)
            },
        ) {
            BoxWithConstraints(if (dedicated) Modifier.fillMaxSize() else Modifier.fillMaxWidth()) {
                val qrSize = if (dedicated) {
                    minOf(maxHeight - 40.dp, maxWidth * 0.36f, 240.dp).coerceAtLeast(152.dp)
                } else {
                    188.dp
                }
                Row(
                    (if (dedicated) Modifier.fillMaxSize() else Modifier.fillMaxWidth())
                        .padding(if (dedicated) 18.dp else 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(if (dedicated) 20.dp else 12.dp),
                ) {
                    if (connector.pairedDeviceName == null) {
                        qrCode?.let { code ->
                            Surface(
                                shape = RoundedCornerShape(18.dp),
                                color = Color.White,
                                border = BorderStroke(3.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.55f)),
                            ) {
                                QrCodeView(code, Modifier.size(qrSize))
                            }
                        }
                    }
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(if (dedicated) 10.dp else 8.dp)) {
                        Text(
                            if (connector.pairedDeviceName == null) "Pair your phone" else "Phone connected",
                            color = TextPrimary,
                            style = if (dedicated) MaterialTheme.typography.headlineMedium else MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold,
                        )
                        Text(
                            if (connector.pairedDeviceName == null) {
                                stringResource(R.string.tv_pair_instructions)
                            } else {
                                "${connector.pairedDeviceName} can launch games. Approve trust below for settings, overlays, sessions, and account switching."
                            },
                            color = TextMuted,
                            style = if (dedicated) MaterialTheme.typography.bodyMedium else MaterialTheme.typography.bodySmall,
                            maxLines = if (connector.pairedDeviceName == null) 4 else if (dedicated) 3 else 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                        if (connector.pairedDeviceName == null) {
                            PairingCodeDisplay(connector.pairingCode, compact = !dedicated)
                        }
                        if (connector.pairedDeviceName != null) {
                            SettingSwitch(
                                label = "Trust this phone",
                                checked = connector.pairedDeviceTrusted,
                                description = "Required before the phone can transfer an account or control TV settings and sessions.",
                            ) { trusted -> viewModel.setLocalTvDeviceTrusted(trusted) }
                        }
                        OutlinedButton(onClick = viewModel::stopLocalTvConnector) {
                            Text(if (connector.pairedDeviceName == null) "Cancel pairing" else "Disconnect phone")
                        }
                    }
                }
            }
        }
    }
    connector.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
}

@Composable
private fun PairingCodeDisplay(code: String?, compact: Boolean) {
    val digits = code?.takeIf { it.length == 4 && it.all(Char::isDigit) } ?: "----"
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(stringResource(R.string.login_pairing_code), color = TextMuted, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.Bold)
        Row(horizontalArrangement = Arrangement.spacedBy(if (compact) 5.dp else 8.dp)) {
            digits.forEach { digit ->
                Surface(
                    modifier = Modifier.size(if (compact) 38.dp else 46.dp),
                    shape = RoundedCornerShape(12.dp),
                    color = Color.White.copy(alpha = 0.07f),
                    border = if (LocalAbsoluteCinemaEffects.current) {
                        BorderStroke(1.dp, LocalActiveSelectionColor.current)
                    } else {
                        null
                    },
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Text(
                            digit.toString(),
                            color = TextPrimary,
                            style = if (compact) MaterialTheme.typography.titleMedium else MaterialTheme.typography.headlineSmall,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun TvDeviceLoginScreen(prompt: DeviceLoginPrompt, phase: String, onCancel: () -> Unit) {
    BoxWithConstraints(
        modifier = Modifier.fillMaxSize().padding(horizontal = 48.dp, vertical = 36.dp),
        contentAlignment = Alignment.Center,
    ) {
        val landscape = maxWidth >= 720.dp
        val qrMaxSize = minOf(
            maxWidth * if (landscape) 0.28f else 0.68f,
            maxHeight * if (landscape) 0.58f else 0.38f,
            340.dp,
        )
        DeviceLoginPanel(
            prompt = prompt,
            phase = phase,
            onCancel = onCancel,
            modifier = Modifier.fillMaxWidth(if (landscape) 0.86f else 1f),
            qrMaxSize = qrMaxSize,
            preferLandscapeLayout = landscape,
            focusCancelOnPrompt = false,
        )
    }
}

@Composable
internal fun DeviceLoginPanel(
    prompt: DeviceLoginPrompt,
    phase: String,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier.fillMaxWidth().padding(horizontal = 24.dp),
    qrMaxSize: androidx.compose.ui.unit.Dp = 360.dp,
    preferLandscapeLayout: Boolean = false,
    focusCancelOnPrompt: Boolean = true,
) {
    val context = LocalContext.current
    val clipboardManager = LocalClipboardManager.current
    val configuration = LocalConfiguration.current
    val initialFocusRequester = remember { FocusRequester() }
    val sideBySideLayout = shouldUseSideBySideDeviceLoginLayout(
        orientation = configuration.orientation,
        preferLandscapeLayout = preferLandscapeLayout,
        availableWidthDp = configuration.screenWidthDp,
    )
    val launchUrl = remember(prompt.verificationUriComplete, prompt.verificationUri) {
        prompt.verificationUriComplete ?: prompt.verificationUri
    }
    val qrContent = launchUrl
    var urlActionMessage by remember(launchUrl) { mutableStateOf<String?>(null) }
    val qrCode = remember(qrContent, prompt.verificationUri) {
        QrCode.encodeText(qrContent) ?: QrCode.encodeText(prompt.verificationUri)
    }
    val remainingSeconds by produceState(initialValue = secondsUntil(prompt.expiresAt), prompt.expiresAt) {
        while (value > 0) {
            delay(1000L)
            value = secondsUntil(prompt.expiresAt)
        }
    }
    LaunchedEffect(prompt.userCode, focusCancelOnPrompt) {
        runCatching { initialFocusRequester.requestFocus() }
    }
    Card(
        colors = CardDefaults.cardColors(containerColor = PanelAlt, contentColor = TextPrimary),
        shape = RoundedCornerShape(14.dp),
        modifier = modifier,
    ) {
        if (sideBySideLayout) {
            Row(
                Modifier.fillMaxWidth().padding(24.dp),
                horizontalArrangement = Arrangement.spacedBy(24.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                DeviceLoginQr(
                    qrCode = qrCode,
                    qrMaxSize = qrMaxSize,
                    modifier = Modifier.weight(0.9f),
                )
                DeviceLoginControls(
                    launchUrl = launchUrl,
                    prompt = prompt,
                    phase = phase,
                    remainingSeconds = remainingSeconds,
                    urlActionMessage = urlActionMessage,
                    onUrlActionMessage = { urlActionMessage = it },
                    onCancel = onCancel,
                    focusRequester = initialFocusRequester,
                    focusCancel = focusCancelOnPrompt,
                    context = context,
                    clipboardManager = clipboardManager,
                    modifier = Modifier.weight(1.1f),
                    showTitle = true,
                )
            }
        } else {
            Column(
                Modifier.padding(20.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(stringResource(R.string.login_tv_title), color = TextPrimary, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                DeviceLoginQr(qrCode = qrCode, qrMaxSize = qrMaxSize)
                DeviceLoginControls(
                    launchUrl = launchUrl,
                    prompt = prompt,
                    phase = phase,
                    remainingSeconds = remainingSeconds,
                    urlActionMessage = urlActionMessage,
                    onUrlActionMessage = { urlActionMessage = it },
                    onCancel = onCancel,
                    focusRequester = initialFocusRequester,
                    focusCancel = focusCancelOnPrompt,
                    context = context,
                    clipboardManager = clipboardManager,
                    showTitle = false,
                )
            }
        }
    }
}

internal fun shouldUseSideBySideDeviceLoginLayout(
    orientation: Int,
    preferLandscapeLayout: Boolean,
    availableWidthDp: Int,
): Boolean =
    preferLandscapeLayout ||
        (orientation == Configuration.ORIENTATION_LANDSCAPE && availableWidthDp >= DEVICE_LOGIN_SIDE_BY_SIDE_MIN_WIDTH_DP)

@Composable
private fun DeviceLoginQr(qrCode: QrCode?, qrMaxSize: androidx.compose.ui.unit.Dp, modifier: Modifier = Modifier) {
    qrCode?.let {
        BoxWithConstraints(
            modifier = modifier.fillMaxWidth(),
            contentAlignment = Alignment.Center,
        ) {
            val qrDisplaySize = minOf(maxWidth * 0.92f, qrMaxSize)
            QrCodeView(it, Modifier.size(qrDisplaySize))
        }
    }
}

@Composable
private fun DeviceLoginControls(
    launchUrl: String,
    prompt: DeviceLoginPrompt,
    phase: String,
    remainingSeconds: Int,
    urlActionMessage: String?,
    onUrlActionMessage: (String) -> Unit,
    onCancel: () -> Unit,
    focusRequester: FocusRequester,
    focusCancel: Boolean,
    context: android.content.Context,
    clipboardManager: androidx.compose.ui.platform.ClipboardManager,
    modifier: Modifier = Modifier,
    showTitle: Boolean = true,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (showTitle) {
            Text(stringResource(R.string.login_tv_title), color = TextPrimary, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        }
        TextButton(
            onClick = {
                val opened = openExternalUrl(context, launchUrl)
                if (opened) {
                    onUrlActionMessage("Opening sign-in URL")
                } else {
                    clipboardManager.setText(AnnotatedString(launchUrl))
                    onUrlActionMessage("URL copied")
                }
            },
            modifier = if (focusCancel) Modifier else Modifier.focusRequester(focusRequester),
        ) {
            Text(
                launchUrl,
                color = MaterialTheme.colorScheme.primary,
                style = MaterialTheme.typography.titleSmall,
                textAlign = TextAlign.Center,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Text(prompt.userCode, style = MaterialTheme.typography.displaySmall, fontWeight = FontWeight.Bold, color = Color.White)
        Text(stringResource(R.string.login_tv_status, phase.ifBlank { stringResource(R.string.login_tv_waiting) }), color = TextMuted)
        urlActionMessage?.let {
            Text(it, color = TextMuted, style = MaterialTheme.typography.bodySmall)
        }
        Text(stringResource(R.string.login_tv_expires, remainingSeconds / 60, remainingSeconds % 60), color = TextMuted)
        OutlinedButton(
            onClick = onCancel,
            modifier = if (focusCancel) Modifier.focusRequester(focusRequester) else Modifier,
        ) {
            Text(stringResource(R.string.action_cancel))
        }
    }
}

internal fun openExternalUrl(context: android.content.Context, url: String): Boolean {
    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    return runCatching {
        context.startActivity(intent)
        true
    }.getOrDefault(false)
}

@Composable
internal fun QrCodeView(qrCode: QrCode, modifier: Modifier = Modifier) {
    Canvas(
        modifier
            .clip(RoundedCornerShape(12.dp))
            .background(Color.White),
    ) {
        val quiet = 4
        val cells = qrCode.size + quiet * 2
        val cellSize = floor(min(size.width, size.height) / cells).coerceAtLeast(1f)
        val qrSize = cellSize * cells
        val originX = floor((size.width - qrSize) / 2f)
        val originY = floor((size.height - qrSize) / 2f)
        for (y in 0 until qrCode.size) {
            for (x in 0 until qrCode.size) {
                if (!qrCode.isDark(x, y)) continue
                drawRect(
                    color = Color.Black,
                    topLeft = Offset(originX + (x + quiet) * cellSize, originY + (y + quiet) * cellSize),
                    size = Size(cellSize, cellSize),
                )
            }
        }
    }
}

private fun secondsUntil(deadlineMs: Long): Int =
    ((deadlineMs - System.currentTimeMillis()).coerceAtLeast(0L) / 1000L).toInt()

internal fun isPhoneLandscape(width: androidx.compose.ui.unit.Dp, height: androidx.compose.ui.unit.Dp): Boolean =
    width > height && minOf(width, height) < PHONE_NAV_RAIL_MAX_SMALLEST_WIDTH

private fun isPhonePortrait(width: androidx.compose.ui.unit.Dp, height: androidx.compose.ui.unit.Dp): Boolean =
    height >= width && minOf(width, height) < PHONE_NAV_RAIL_MAX_SMALLEST_WIDTH

@Composable
internal fun rememberPhysicalControllerConnected(enabled: Boolean): Boolean {
    return rememberPhysicalControllerFamily(enabled) != null
}

@Composable
internal fun rememberPhysicalControllerFamily(enabled: Boolean): AndroidControllerFamily? {
    val context = LocalContext.current.applicationContext
    var family by remember { mutableStateOf(connectedPhysicalControllerFamily().takeIf { enabled }) }
    DisposableEffect(context, enabled) {
        fun refresh() {
            family = connectedPhysicalControllerFamily().takeIf { enabled }
        }
        refresh()
        if (!enabled) {
            onDispose {}
        } else {
            val inputManager = context.getSystemService(Context.INPUT_SERVICE) as? InputManager
            val listener = object : InputManager.InputDeviceListener {
                override fun onInputDeviceAdded(deviceId: Int) = refresh()
                override fun onInputDeviceRemoved(deviceId: Int) = refresh()
                override fun onInputDeviceChanged(deviceId: Int) = refresh()
            }
            inputManager?.registerInputDeviceListener(listener, null)
            onDispose {
                inputManager?.unregisterInputDeviceListener(listener)
            }
        }
    }
    return family
}

private fun connectedPhysicalControllerFamily(): AndroidControllerFamily? {
    val families = InputDevice.getDeviceIds()
        .asSequence()
        .mapNotNull { deviceId -> AndroidControllerInput.controllerFamily(InputDevice.getDevice(deviceId)) }
        .toList()
    return families.firstOrNull { it != AndroidControllerFamily.Generic } ?: families.firstOrNull()
}
