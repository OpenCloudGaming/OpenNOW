package com.opencloudgaming.opennow

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.Alignment
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlin.math.roundToInt

internal val SettingsBackground = Color(0xff090b0d)
internal val SettingsPanel = Color(0xff11161a)
internal val SettingsPanelAlt = Color(0xff171d22)
internal val SettingsText = Color(0xffeef3f5)
internal val SettingsTextMuted = Color(0xff98a4aa)
internal const val DONATE_URL = "https://paypal.me/PrintedWaste"
internal val PHONE_NAV_RAIL_MAX_SMALLEST_WIDTH = 600.dp
internal val APP_NAV_RAIL_WIDTH = 80.dp
internal const val PHONE_ULTRAWIDE_MIN_STREAM_ASPECT = 2.2f
internal const val PHONE_ULTRAWIDE_MIN_VIEWPORT_ASPECT = 2.0f

internal data class SettingsChoiceOption(val value: String, val label: String)
internal data class ChoiceMenuOption(
    val value: String,
    val label: String,
    val enabled: Boolean = true,
    val badge: String? = null,
)

internal enum class SearchTarget {
    Store,
    Library,
    Settings,
}

private enum class SettingsCategory(
    val title: String,
    val summary: String,
    val iconRes: Int,
) {
    General("General", "Updates, privacy, cache, and reset", R.drawable.ic_tab_settings),
    Stream("Stream", "Resolution, FPS, codec, HDR, proxy", R.drawable.ic_tab_stream),
    Input("Input", "Mouse, keyboard, touch controls, rumble", R.drawable.ic_tab_library),
    Interface("Interface", "Color, cards, stats, controller UI", R.drawable.ic_tab_store),
    Account("Account", "Sign-in, storage, connected stores", R.drawable.ic_tab_store),
    Advanced("Advanced", "Diagnostics, debug logs, nerd tools", R.drawable.ic_search),
    About("About", "Version, credits, and support", R.drawable.ic_tab_settings),
}

internal data class LauncherBadge(
    val iconRes: Int,
    val name: String,
    val background: Color,
    val foreground: Color = SettingsText,
)

private val keyboardLayoutOptions = listOf(
    SettingsChoiceOption("en-US", "English (US)"),
    SettingsChoiceOption("en-GB", "English (UK)"),
    SettingsChoiceOption("tr-TR", "Turkish Q"),
    SettingsChoiceOption("de-DE", "German"),
    SettingsChoiceOption("fr-FR", "French"),
    SettingsChoiceOption("es-ES", "Spanish"),
    SettingsChoiceOption("es-MX", "Spanish (Latin America)"),
    SettingsChoiceOption("it-IT", "Italian"),
    SettingsChoiceOption("pt-PT", "Portuguese (Portugal)"),
    SettingsChoiceOption("pt-BR", "Portuguese (Brazil)"),
    SettingsChoiceOption("pl-PL", "Polish"),
    SettingsChoiceOption("ru-RU", "Russian"),
    SettingsChoiceOption("ja-JP", "Japanese"),
    SettingsChoiceOption("ko-KR", "Korean"),
    SettingsChoiceOption("zh-CN", "Chinese (Simplified)"),
    SettingsChoiceOption("zh-TW", "Chinese (Traditional)"),
)

private val gameLanguageOptions = listOf(
    SettingsChoiceOption("en_US", "English (US)"),
    SettingsChoiceOption("en_GB", "English (UK)"),
    SettingsChoiceOption("de_DE", "Deutsch"),
    SettingsChoiceOption("fr_FR", "Francais"),
    SettingsChoiceOption("es_ES", "Espanol (ES)"),
    SettingsChoiceOption("es_MX", "Espanol (MX)"),
    SettingsChoiceOption("it_IT", "Italiano"),
    SettingsChoiceOption("pt_PT", "Portugues (PT)"),
    SettingsChoiceOption("pt_BR", "Portugues (BR)"),
    SettingsChoiceOption("ru_RU", "Russian"),
    SettingsChoiceOption("pl_PL", "Polish"),
    SettingsChoiceOption("tr_TR", "Turkish"),
    SettingsChoiceOption("ar_SA", "Arabic"),
    SettingsChoiceOption("ja_JP", "Japanese"),
    SettingsChoiceOption("ko_KR", "Korean"),
    SettingsChoiceOption("zh_CN", "Chinese (Simplified)"),
    SettingsChoiceOption("zh_TW", "Chinese (Traditional)"),
    SettingsChoiceOption("th_TH", "Thai"),
    SettingsChoiceOption("vi_VN", "Vietnamese"),
    SettingsChoiceOption("id_ID", "Indonesian"),
    SettingsChoiceOption("cs_CZ", "Czech"),
    SettingsChoiceOption("el_GR", "Greek"),
    SettingsChoiceOption("hu_HU", "Hungarian"),
    SettingsChoiceOption("ro_RO", "Romanian"),
    SettingsChoiceOption("uk_UA", "Ukrainian"),
    SettingsChoiceOption("nl_NL", "Dutch"),
    SettingsChoiceOption("sv_SE", "Swedish"),
    SettingsChoiceOption("da_DK", "Danish"),
    SettingsChoiceOption("fi_FI", "Finnish"),
    SettingsChoiceOption("no_NO", "Norwegian"),
)

@Composable
internal fun SettingsScreen(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    tvProfile: Boolean,
    searchRequested: Boolean,
    searchQuery: String,
    onSearchQueryChange: (String) -> Unit,
) {
    var showSessionProxyWarning by remember { mutableStateOf(false) }
    var selectedCategory by remember { mutableStateOf<SettingsCategory?>(null) }
    val scrollState = rememberScrollState()
    val searchFocusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    val focusManager = LocalFocusManager.current
    val showSearch = searchRequested || searchQuery.isNotBlank()
    val categories = remember(state.settings.nerdMode) { settingsCategories(state.settings.nerdMode) }
    LaunchedEffect(searchRequested) {
        if (searchRequested) {
            delay(90)
            runCatching { searchFocusRequester.requestFocus() }
            keyboardController?.show()
        }
    }
    LaunchedEffect(categories) {
        if (selectedCategory != null && selectedCategory !in settingsDetailCategories(state.settings.nerdMode)) {
            selectedCategory = null
        }
    }
    BackHandler(enabled = selectedCategory != null) {
        selectedCategory = null
    }
    if (showSessionProxyWarning) {
        SessionProxyWarningDialog(
            onCancel = { showSessionProxyWarning = false },
            onEnable = {
                viewModel.updateStreamSettings { s -> s.copy(sessionProxyEnabled = true) }
                showSessionProxyWarning = false
            },
        )
    }
    if (tvProfile) {
        Column(
            Modifier
                .fillMaxSize()
                .background(SettingsBackground)
                .onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) }
                .verticalScroll(scrollState)
                .padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            AnimatedVisibility(visible = showSearch) {
                NativeSearchField(
                    query = searchQuery,
                    onQueryChange = onSearchQueryChange,
                    placeholder = stringResource(R.string.search_settings),
                    focusRequester = searchFocusRequester,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            AnimatedContent(targetState = selectedCategory, label = "settings-route") { category ->
                SettingsBody(
                    state = state,
                    viewModel = viewModel,
                    searchQuery = searchQuery,
                    selectedCategory = category,
                    categories = categories,
                    onSelectCategory = { selectedCategory = it },
                    onBack = { selectedCategory = null },
                    showSessionProxyWarning = { showSessionProxyWarning = true },
                )
            }
        }
    } else {
        LazyColumn(
            Modifier
                .fillMaxSize()
                .background(SettingsBackground),
            contentPadding = PaddingValues(14.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                AnimatedVisibility(visible = showSearch) {
                    NativeSearchField(
                        query = searchQuery,
                        onQueryChange = onSearchQueryChange,
                        placeholder = stringResource(R.string.search_settings),
                        focusRequester = searchFocusRequester,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
            item {
                AnimatedContent(targetState = selectedCategory, label = "settings-route") { category ->
                    SettingsBody(
                        state = state,
                        viewModel = viewModel,
                        searchQuery = searchQuery,
                        selectedCategory = category,
                        categories = categories,
                        onSelectCategory = { selectedCategory = it },
                        onBack = { selectedCategory = null },
                        showSessionProxyWarning = { showSessionProxyWarning = true },
                    )
                }
            }
        }
    }
}

@Composable
private fun SettingsBody(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    searchQuery: String,
    selectedCategory: SettingsCategory?,
    categories: List<SettingsCategory>,
    onSelectCategory: (SettingsCategory) -> Unit,
    onBack: () -> Unit,
    showSessionProxyWarning: () -> Unit,
) {
    when {
        searchQuery.isNotBlank() -> {
            SettingsContent(
                state = state,
                viewModel = viewModel,
                searchQuery = searchQuery,
                selectedCategory = null,
                showSessionProxyWarning = showSessionProxyWarning,
            )
        }
        selectedCategory == null -> {
            SettingsCategoryLanding(
                state = state,
                categories = categories,
                onSelectCategory = onSelectCategory,
            )
        }
        else -> {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                SettingsDetailHeader(category = selectedCategory, onBack = onBack)
                SettingsContent(
                    state = state,
                    viewModel = viewModel,
                    searchQuery = searchQuery,
                    selectedCategory = selectedCategory,
                    showSessionProxyWarning = showSessionProxyWarning,
                )
            }
        }
    }
}

@Composable
private fun SettingsContent(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    searchQuery: String,
    selectedCategory: SettingsCategory?,
    showSessionProxyWarning: () -> Unit,
) {
    val settings = state.settings
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
    CategorySettingsSection(selectedCategory, SettingsCategory.General, searchQuery, "App updates", "update", "updates", "disable update checking", "checking", "check", "download", "install", "apk") {
                SettingSwitch(stringResource(R.string.settings_disable_update_checking), !settings.autoCheckForUpdates) { disabled ->
                    viewModel.updateSettings(settings.copy(autoCheckForUpdates = !disabled))
                }
                AndroidUpdatePanel(state = state, viewModel = viewModel)
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.General, searchQuery, "Privacy", "privacy", "analytics", "telemetry", "posthog", "usage", "tracking", "opt out") {
                SettingSwitch("Share usage analytics", !settings.analyticsOptOut) { enabled ->
                    viewModel.updateSettings(settings.copy(analyticsOptOut = !enabled))
                }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Stream, searchQuery, stringResource(R.string.settings_section_stream), "stream", "resolution", "aspect ratio", "fps", "bitrate", "codec", "color", "hdr", "sharpening", "region", "session proxy", "proxy", "l4s", "cloud g-sync", "vrr") {
                val fallbackMembershipTier = state.authSession?.user?.membershipTier
                val resolutionChoices = streamResolutionChoicesForAspect(settings.stream.aspectRatio).ifEmpty {
                    streamResolutionChoicesForAspect("16:9")
                }
                val selectedResolution = normalizeStreamResolutionForAspectAndPlan(
                    settings.stream.resolution,
                    settings.stream.aspectRatio,
                    state.subscriptionInfo,
                    fallbackMembershipTier,
                )
                ChoiceMenuRow(
                    label = stringResource(R.string.settings_resolution),
                    options = resolutionChoices.map { choice ->
                        val available = choice.isAvailableFor(state.subscriptionInfo, fallbackMembershipTier)
                        ChoiceMenuOption(
                            value = choice.value,
                            label = choice.label,
                            enabled = available,
                            badge = if (available) null else choice.requiredPlanLabel,
                        )
                    },
                    selectedLabel = resolutionChoices.firstOrNull { it.value == selectedResolution }?.label ?: selectedResolution,
                ) {
                    viewModel.updateStreamSettings { s -> s.copy(resolution = it) }
                }
                ChoiceMenuRow(
                    label = stringResource(R.string.settings_aspect_ratio),
                    options = streamAspectRatioOptions().map { aspectRatio ->
                        val choices = streamResolutionChoicesForAspect(aspectRatio)
                        val available = choices.any { it.isAvailableFor(state.subscriptionInfo, fallbackMembershipTier) }
                        ChoiceMenuOption(
                            value = aspectRatio,
                            label = aspectRatio,
                            enabled = available,
                            badge = if (available) null else choices.firstNotNullOfOrNull { it.requiredPlanLabel },
                        )
                    },
                    selectedLabel = settings.stream.aspectRatio,
                ) {
                    viewModel.updateStreamSettings { s ->
                        s.copy(
                            aspectRatio = it,
                            resolution = normalizeStreamResolutionForAspectAndPlan(
                                s.resolution,
                                it,
                                state.subscriptionInfo,
                                fallbackMembershipTier,
                            ),
                        )
                    }
                }
                SettingSwitch(stringResource(R.string.settings_stretch_stream_to_fill), settings.stretchStreamToFill) { enabled ->
                    viewModel.updateSettings(settings.copy(stretchStreamToFill = enabled))
                }
                NumberSlider(stringResource(R.string.settings_fps), settings.stream.fps.toFloat(), 30f, 240f, 30f) {
                    viewModel.updateStreamSettings { s -> s.copy(fps = it.roundToInt()) }
                }
                NumberSlider(stringResource(R.string.settings_bitrate), settings.stream.maxBitrateMbps.toFloat(), 1f, 150f, 1f) {
                    viewModel.updateStreamSettings { s -> s.copy(maxBitrateMbps = it.roundToInt()) }
                }
                val effectiveCodec = settings.stream.adjustedForDevice(state.codecReport).codec
                ChoiceMenuRow(
                    label = stringResource(R.string.settings_codec),
                    options = VideoCodec.entries.map { codec ->
                        val launchUsable = state.codecReport
                            ?.capabilities
                            ?.firstOrNull { it.codec == codec }
                            ?.streamingDecoderUsableForLaunch()
                            ?: true
                        ChoiceMenuOption(
                            value = codec.name,
                            label = codec.name,
                            enabled = launchUsable,
                            badge = if (launchUsable) null else "Unavailable",
                        )
                    },
                    selectedLabel = if (effectiveCodec == settings.stream.codec) {
                        settings.stream.codec.name
                    } else {
                        "${settings.stream.codec.name} -> ${effectiveCodec.name}"
                    },
                ) {
                    viewModel.updateStreamSettings { s -> s.copy(codec = VideoCodec.valueOf(it)) }
                }
                ChoiceRow(stringResource(R.string.settings_color), ColorQuality.entries.map { it.label }, settings.stream.colorQuality.label) { label ->
                    viewModel.updateStreamSettings { s -> s.copy(colorQuality = ColorQuality.entries.first { it.label == label }) }
                }
                val hdrAvailable = hasUltimateStreamingPlan(state.subscriptionInfo, fallbackMembershipTier)
                SettingSwitch(
                    stringResource(R.string.settings_hdr),
                    settings.stream.hdrEnabled && hdrAvailable,
                    enabled = hdrAvailable,
                ) { enabled ->
                    viewModel.updateStreamSettings { s ->
                        s.copy(
                            hdrEnabled = enabled,
                            colorQuality = if (enabled && !s.colorQuality.name.startsWith("TenBit")) ColorQuality.TenBit420 else s.colorQuality,
                        )
                    }
                }
                SettingSwitch("Stream sharpening", settings.stream.streamSharpeningEnabled) {
                    viewModel.updateStreamSettings { s -> s.copy(streamSharpeningEnabled = it) }
                }
                if (settings.stream.streamSharpeningEnabled) {
                    NumberSlider("Sharpness amount", settings.stream.streamSharpeningAmount, 0f, 1f, 0.05f) {
                        viewModel.updateStreamSettings { s -> s.copy(streamSharpeningAmount = it) }
                    }
                }
                ChoiceRow(stringResource(R.string.settings_region), listOf(stringResource(R.string.option_auto)) + state.regions.map { it.name }, state.regions.firstOrNull { it.url == settings.stream.region }?.name ?: stringResource(R.string.option_auto)) { label ->
                    val url = state.regions.firstOrNull { it.name == label }?.url.orEmpty()
                    viewModel.updateStreamSettings { s -> s.copy(region = url) }
                }
                SettingSwitch(stringResource(R.string.settings_session_proxy), settings.stream.sessionProxyEnabled) { enabled ->
                    if (enabled) {
                        showSessionProxyWarning()
                    } else {
                        viewModel.updateStreamSettings { s -> s.copy(sessionProxyEnabled = false) }
                    }
                }
                Text(
                    stringResource(R.string.settings_session_proxy_hint),
                    color = SettingsTextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
                if (settings.stream.sessionProxyEnabled) {
                    OutlinedTextField(
                        value = settings.stream.sessionProxyUrl,
                        onValueChange = { value -> viewModel.updateStreamSettings { s -> s.copy(sessionProxyUrl = value) } },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true,
                        label = { Text(stringResource(R.string.settings_session_proxy_url)) },
                        placeholder = { Text("http://127.0.0.1:8080") },
                    )
                }
                SettingSwitch(stringResource(R.string.settings_l4s), settings.stream.enableL4S) { viewModel.updateStreamSettings { s -> s.copy(enableL4S = it) } }
                SettingSwitch(stringResource(R.string.settings_cloud_gsync), settings.stream.enableCloudGsync) { viewModel.updateStreamSettings { s -> s.copy(enableCloudGsync = it) } }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Input, searchQuery, "Input", "input", "mouse", "sensitivity", "acceleration", "keyboard", "layout", "language", "clipboard", "paste", "rumble", "touch", "finger", "opacity", "edge", "padding", "offset", "controls", "stick", "button", "tone") {
                NumberSlider("Mouse sensitivity", settings.stream.mouseSensitivity, 0.25f, 3f, 0.05f) {
                    viewModel.updateStreamSettings { s -> s.copy(mouseSensitivity = it) }
                }
                NumberSlider("Mouse acceleration", settings.stream.mouseAcceleration.toFloat(), 1f, 150f, 1f) {
                    viewModel.updateStreamSettings { s -> s.copy(mouseAcceleration = it.roundToInt()) }
                }
                ChoiceOptionRow("Keyboard layout", keyboardLayoutOptions, settings.stream.keyboardLayout) {
                    viewModel.updateStreamSettings { s -> s.copy(keyboardLayout = it) }
                }
                ChoiceOptionRow("Game language", gameLanguageOptions, settings.stream.gameLanguage) {
                    viewModel.updateStreamSettings { s -> s.copy(gameLanguage = it) }
                }
                SettingSwitch("Clipboard paste", settings.clipboardPaste) { enabled -> viewModel.updateSettings(settings.copy(clipboardPaste = enabled)) }
                SettingSwitch("Phone rumble fallback", settings.phoneRumbleFallback) { enabled -> viewModel.updateSettings(settings.copy(phoneRumbleFallback = enabled)) }
                SettingSwitch("Touch controls", settings.androidTouch.enabled) { enabled -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(enabled = enabled))) }
                SettingSwitch("Finger mouse", settings.androidTouch.mousePad) { enabled -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(mousePad = enabled))) }
                NumberSlider("Touch layout scale", settings.androidTouch.scale, 0.6f, 1.4f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(scale = value))) }
                NumberSlider("Touch button size", settings.androidTouch.buttonScale, 0.65f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(buttonScale = value))) }
                NumberSlider("Touch stick size", settings.androidTouch.stickScale, 0.65f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(stickScale = value))) }
                NumberSlider("Touch opacity", settings.androidTouch.opacity, 0.15f, 1f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(opacity = value))) }
                NumberSlider("Touch edge padding", settings.androidTouch.edgePaddingDp, 0f, 72f, 1f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(edgePaddingDp = value))) }
                NumberSlider("Touch bottom padding", settings.androidTouch.bottomPaddingDp, 0f, 120f, 1f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(bottomPaddingDp = value))) }
                NumberSlider("Left controls horizontal offset", settings.androidTouch.leftOffsetXDp, -220f, 220f, 2f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(leftOffsetXDp = value))) }
                NumberSlider("Left controls vertical offset", settings.androidTouch.leftOffsetYDp, -160f, 160f, 2f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(leftOffsetYDp = value))) }
                NumberSlider("Right controls horizontal offset", settings.androidTouch.rightOffsetXDp, -220f, 220f, 2f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(rightOffsetXDp = value))) }
                NumberSlider("Right controls vertical offset", settings.androidTouch.rightOffsetYDp, -160f, 160f, 2f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(rightOffsetYDp = value))) }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Interface, searchQuery, stringResource(R.string.settings_section_interface), "interface", "ui", "system colors", "accent", "nerd", "expressive", "compact", "cards", "store labels", "game card size", "stats", "server selector", "controller", "sounds", "animations", "backdrop", "auto-load", "library", "session counter", "intro", "music", "queue", "stretch", "fill") {
                val accentOptions = UiAccent.entries.map { it to uiAccentLabel(it) }
                SettingSwitch(stringResource(R.string.settings_dynamic_color), settings.dynamicColor) { viewModel.updateSettings(settings.copy(dynamicColor = it)) }
                ChoiceRow(stringResource(R.string.settings_accent), accentOptions.map { it.second }, accentOptions.firstOrNull { it.first == settings.uiAccent }?.second ?: accentOptions.first().second) { label ->
                    accentOptions.firstOrNull { it.second == label }?.first?.let { accent ->
                        viewModel.updateSettings(settings.copy(uiAccent = accent))
                    }
                }
                SettingSwitch(stringResource(R.string.settings_nerd_mode), settings.nerdMode) { viewModel.updateSettings(settings.copy(nerdMode = it)) }
                SettingSwitch(stringResource(R.string.settings_expressive_ui), settings.expressiveUi) { viewModel.updateSettings(settings.copy(expressiveUi = it)) }
                SettingSwitch(stringResource(R.string.settings_compact_cards), settings.compactGameCards) { viewModel.updateSettings(settings.copy(compactGameCards = it)) }
                SettingSwitch(stringResource(R.string.settings_show_store_labels), settings.showGameStoreLabels) { viewModel.updateSettings(settings.copy(showGameStoreLabels = it)) }
                NumberSlider(stringResource(R.string.settings_card_size), settings.posterSizeScale, 0.82f, 1.08f, 0.02f) { value -> viewModel.updateSettings(settings.copy(posterSizeScale = value)) }
                SettingSwitch(stringResource(R.string.settings_show_stats), settings.showStatsOnLaunch) { viewModel.updateSettings(settings.copy(showStatsOnLaunch = it)) }
                ChoiceRow("Stats overlay style", StreamStatsStyle.entries.map { it.label }, settings.streamStatsStyle.label) { label ->
                    StreamStatsStyle.entries.firstOrNull { it.label == label }?.let { style ->
                        viewModel.updateSettings(settings.copy(streamStatsStyle = style))
                    }
                }
                SettingSwitch(stringResource(R.string.settings_hide_server_selector), settings.hideServerSelector) { viewModel.updateSettings(settings.copy(hideServerSelector = it)) }
                SettingSwitch(stringResource(R.string.settings_controller_mode), settings.controllerMode) { viewModel.updateSettings(settings.copy(controllerMode = it)) }
                SettingSwitch(stringResource(R.string.settings_controller_animations), settings.controllerBackgroundAnimations) { viewModel.updateSettings(settings.copy(controllerBackgroundAnimations = it)) }
                SettingSwitch(stringResource(R.string.settings_controller_backdrop), settings.controllerLibraryGameBackdrop) { viewModel.updateSettings(settings.copy(controllerLibraryGameBackdrop = it)) }
                SettingSwitch(stringResource(R.string.settings_auto_load_library), settings.autoLoadControllerLibrary) { viewModel.updateSettings(settings.copy(autoLoadControllerLibrary = it)) }
                SettingSwitch(stringResource(R.string.settings_session_counter), settings.sessionCounterEnabled) { viewModel.updateSettings(settings.copy(sessionCounterEnabled = it)) }
                SettingSwitch(stringResource(R.string.settings_stream_intro_music), settings.streamIntroMusic) { enabled ->
                    viewModel.updateSettings(settings.copy(streamIntroMusic = enabled))
                }
                SettingSwitch(stringResource(R.string.settings_queue_ready_music), settings.queueReadyMusic) { enabled ->
                    viewModel.updateSettings(settings.copy(queueReadyMusic = enabled))
                }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.General, searchQuery, "App Data", "app data", "data", "cache", "clear", "reset", "settings") {
                AppDataSettingsPanel(viewModel = viewModel)
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Account, searchQuery, "Account", "account", "login", "logout", "sign in", "saved", "provider", "membership", "subscription") {
                AccountSettingsPanel(state = state, viewModel = viewModel)
            }
    if (settings.nerdMode) {
    CategorySettingsSection(selectedCategory, SettingsCategory.Advanced, searchQuery, stringResource(R.string.settings_section_nerd_tools), "nerd", "intro", "music", "rock", "tone", "button", "sound", "stretch", "fill", "fullscreen", "wave") {
                    SettingSwitch(stringResource(R.string.settings_button_press_tones), settings.controllerUiSounds) { enabled ->
                        viewModel.updateSettings(settings.copy(controllerUiSounds = enabled))
                    }
                }
    CategorySettingsSection(selectedCategory, SettingsCategory.Advanced, searchQuery, "Codec Diagnostics", "codec", "diagnostics", "probe", "av1", "h264", "h265", "hevc", "decode") {
                    CodecDiagnosticsPanel(state.codecReport)
                }
    CategorySettingsSection(selectedCategory, SettingsCategory.Advanced, searchQuery, "Debug Logs", "debug", "logs", "logcat", "events") {
                    DebugLogsPanel(state = state, viewModel = viewModel)
                }
    }
    CategorySettingsSection(selectedCategory, SettingsCategory.About, searchQuery, "About", "about", "version", "build", "app") {
                AppVersionPanel()
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.About, searchQuery, stringResource(R.string.settings_section_thanks), "thanks", "credits", "contributors", "darkevilpt", "donate", "paypal", "printedwaste") {
                ThanksPanel()
            }
    }
}

@Composable
private fun SettingsCategoryLanding(
    state: OpenNowUiState,
    categories: List<SettingsCategory>,
    onSelectCategory: (SettingsCategory) -> Unit,
) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        SettingsAccountCard(
            state = state,
            onClick = { onSelectCategory(SettingsCategory.Account) },
        )
        Surface(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(28.dp),
            color = MaterialTheme.colorScheme.surface,
        ) {
            Column(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
                categories.forEach { category ->
                    SettingsCategoryRow(
                        category = category,
                        onClick = { onSelectCategory(category) },
                    )
                }
            }
        }
        SearchableSettingsSection("", "About", "about", "version", "build", "app") {
            AppVersionPanel()
        }
        SearchableSettingsSection("", stringResource(R.string.settings_section_thanks), "thanks", "credits", "contributors", "darkevilpt", "donate", "paypal", "printedwaste") {
            ThanksPanel()
        }
    }
}

@Composable
private fun SettingsAccountCard(state: OpenNowUiState, onClick: () -> Unit) {
    val account = state.savedAccounts.firstOrNull { it.userId == state.authSession?.user?.userId }
        ?: state.savedAccounts.firstOrNull()
    val displayName = account?.displayName?.takeIf { it.isNotBlank() }
        ?: state.authSession?.user?.displayName?.takeIf { it.isNotBlank() }
        ?: "NVIDIA Account"
    val email = account?.email?.takeIf { it.isNotBlank() }
        ?: state.authSession?.user?.email?.takeIf { it.isNotBlank() }
    val tier = state.subscriptionInfo?.membershipTier?.takeIf { it.isNotBlank() }
        ?: account?.membershipTier?.takeIf { it.isNotBlank() }
        ?: state.authSession?.user?.membershipTier?.takeIf { it.isNotBlank() }
    val detail = listOfNotNull(email, tier).joinToString(" - ").ifBlank {
        if (state.authSession == null && account == null) "Sign in to sync your GeForce NOW account" else "Manage account"
    }
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(28.dp))
            .clickable(onClick = onClick),
        shape = RoundedCornerShape(28.dp),
        color = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 18.dp, vertical = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Surface(
                modifier = Modifier.size(52.dp),
                shape = CircleShape,
                color = MaterialTheme.colorScheme.primary,
            ) {
                Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxSize()) {
                    Text(
                        displayName.firstOrNull()?.uppercaseChar()?.toString() ?: "N",
                        color = MaterialTheme.colorScheme.onPrimary,
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.Bold,
                        textAlign = TextAlign.Center,
                    )
                }
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    displayName,
                    color = SettingsText,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    detail,
                    color = SettingsTextMuted,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Icon(
                painter = painterResource(R.drawable.ic_chevron_right),
                contentDescription = null,
                tint = SettingsTextMuted,
                modifier = Modifier.size(22.dp),
            )
        }
    }
}

@Composable
private fun SettingsCategoryRow(category: SettingsCategory, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 20.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        Surface(
            modifier = Modifier.size(42.dp),
            shape = RoundedCornerShape(14.dp),
            color = MaterialTheme.colorScheme.primary.copy(alpha = 0.16f),
        ) {
            Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxSize()) {
                Icon(
                    painter = painterResource(category.iconRes),
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(22.dp),
                )
            }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                category.title,
                color = SettingsText,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                category.summary,
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodyMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Icon(
            painter = painterResource(R.drawable.ic_chevron_right),
            contentDescription = null,
            tint = SettingsTextMuted,
            modifier = Modifier.size(22.dp),
        )
    }
}

@Composable
private fun SettingsDetailHeader(category: SettingsCategory, onBack: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        IconButton(onClick = onBack) {
            Icon(
                painter = painterResource(R.drawable.ic_arrow_back),
                contentDescription = "Back",
                tint = SettingsText,
            )
        }
        Column(Modifier.weight(1f)) {
            Text(
                category.title,
                color = SettingsText,
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                category.summary,
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodyMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun CategorySettingsSection(
    selectedCategory: SettingsCategory?,
    category: SettingsCategory,
    searchQuery: String,
    title: String,
    vararg keywords: String,
    content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit,
) {
    if (searchQuery.isNotBlank() || selectedCategory == null || selectedCategory == category) {
        SearchableSettingsSection(searchQuery, title, *keywords, content = content)
    }
}

private fun settingsCategories(nerdMode: Boolean): List<SettingsCategory> =
    buildList {
        add(SettingsCategory.General)
        add(SettingsCategory.Stream)
        add(SettingsCategory.Input)
        add(SettingsCategory.Interface)
        if (nerdMode) add(SettingsCategory.Advanced)
    }

private fun settingsDetailCategories(nerdMode: Boolean): List<SettingsCategory> =
    settingsCategories(nerdMode) + SettingsCategory.Account
