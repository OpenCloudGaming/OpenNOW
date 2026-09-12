package com.opencloudgaming.opennow

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.gestures.BringIntoViewSpec
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.annotation.StringRes
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.DeveloperMode
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Monitor
import androidx.compose.material.icons.outlined.Palette
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material.icons.outlined.Science
import androidx.compose.material.icons.outlined.SportsEsports
import androidx.compose.material.icons.outlined.Tune
import androidx.compose.material.icons.outlined.Tv
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.produceState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.Alignment
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import com.opencloudgaming.opennow.ui.controls.ControlActionRow
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowMotion
import com.opencloudgaming.opennow.ui.theme.OpenNowPalette
import kotlinx.coroutines.withContext
import kotlin.math.roundToInt

// Aliases onto the shared token layer — these used to be a byte-for-byte copy of the palette in
// OpenNowScreens.kt, which meant any colour change had to be made twice or the two would drift.
internal val SettingsPanel = OpenNowPalette.Panel
internal val SettingsPanelAlt = OpenNowPalette.PanelAlt
internal val SettingsText = OpenNowPalette.TextPrimary
internal val SettingsTextMuted = OpenNowPalette.TextMuted
internal const val DONATE_URL = "https://printedwaste.com/donate"
internal val PHONE_NAV_RAIL_MAX_SMALLEST_WIDTH = 600.dp
internal val APP_NAV_RAIL_WIDTH = 80.dp
internal const val PHONE_ULTRAWIDE_MIN_STREAM_ASPECT = 2.2f
internal const val PHONE_ULTRAWIDE_MIN_VIEWPORT_ASPECT = 2.0f
internal val LocalSettingsControllerNavigationEnabled = androidx.compose.runtime.staticCompositionLocalOf { false }
private val SettingsFocusTopClearance = 16.dp
private val SettingsFocusBottomClearance = 40.dp

internal fun settingsFocusScrollDistance(
    itemOffsetPx: Float,
    itemSizePx: Float,
    containerSizePx: Float,
    topClearancePx: Float,
    bottomClearancePx: Float,
): Float {
    if (itemSizePx <= 0f || containerSizePx <= 0f || itemSizePx >= containerSizePx) return 0f
    val availableClearance = (containerSizePx - itemSizePx).coerceAtLeast(0f)
    val requestedClearance = topClearancePx.coerceAtLeast(0f) + bottomClearancePx.coerceAtLeast(0f)
    val clearanceScale = if (requestedClearance > availableClearance && requestedClearance > 0f) {
        availableClearance / requestedClearance
    } else {
        1f
    }
    val safeTop = topClearancePx.coerceAtLeast(0f) * clearanceScale
    val safeBottom = containerSizePx - bottomClearancePx.coerceAtLeast(0f) * clearanceScale
    val itemBottom = itemOffsetPx + itemSizePx
    return when {
        itemOffsetPx < safeTop -> itemOffsetPx - safeTop
        itemBottom > safeBottom -> itemBottom - safeBottom
        else -> 0f
    }
}

internal data class SettingsChoiceOption(val value: String, val label: String)
internal data class ChoiceMenuOption(
    val value: String,
    val label: String,
    val enabled: Boolean = true,
    val badge: String? = null,
)

internal data class AndroidCodecChoicePresentation(
    val options: List<ChoiceMenuOption>,
    val selectedLabel: String,
)

/** One codec availability presentation shared by Settings and first-run custom setup. */
internal fun androidCodecChoicePresentation(
    stream: StreamSettings,
    codecReport: RuntimeCodecReport?,
    comingSoonLabel: String,
    unavailableLabel: String,
): AndroidCodecChoicePresentation {
    val settingsAvailableStream = stream.withAndroidSettingsAvailability()
    val effectiveCodec = settingsAvailableStream.adjustedForDevice(codecReport).codec
    return AndroidCodecChoicePresentation(
        options = VideoCodec.entries.map { codec ->
            val launchUsable = codecReport
                ?.capabilities
                ?.firstOrNull { it.codec == codec }
                ?.streamingDecoderUsableForLaunch()
                ?: true
            val settingsAvailable = codec.availableForAndroidSettings()
            val available = settingsAvailable && launchUsable
            ChoiceMenuOption(
                value = codec.name,
                label = codec.name,
                enabled = available,
                badge = when {
                    available -> null
                    !settingsAvailable -> comingSoonLabel
                    else -> unavailableLabel
                },
            )
        },
        selectedLabel = if (effectiveCodec == stream.codec) {
            stream.codec.name
        } else {
            "${stream.codec.name} -> ${effectiveCodec.name}"
        },
    )
}

internal enum class SearchTarget {
    Store,
    Library,
    Settings,
}

/**
 * Titles and summaries are string resources rather than hardcoded English constants, so Android's
 * app-owned `res/values-*` translations can cover them without a runtime translation table.
 *
 * Icons come from `material-icons-extended` (already a dependency) so each category gets a distinct
 * one. The previous set reused `ic_tab_store` for both Interface and Account, `ic_tab_settings` for
 * both General and About, and a magnifying glass for Advanced.
 */
private enum class SettingsCategory(
    @StringRes val titleRes: Int,
    @StringRes val summaryRes: Int,
    val icon: ImageVector,
) {
    General(R.string.settings_category_general, R.string.settings_category_general_summary, Icons.Outlined.Tune),
    Stream(R.string.settings_category_stream, R.string.settings_category_stream_summary, Icons.Outlined.Monitor),
    Input(R.string.settings_category_input, R.string.settings_category_input_summary, Icons.Outlined.SportsEsports),
    Interface(R.string.settings_category_interface, R.string.settings_category_interface_summary, Icons.Outlined.Palette),
    Account(R.string.settings_category_account, R.string.settings_category_account_summary, Icons.Outlined.Person),
    TvPairing(R.string.tv_pair_settings_title, R.string.tv_pair_settings_summary, Icons.Outlined.Tv),
    Advanced(R.string.settings_category_advanced, R.string.settings_category_advanced_summary, Icons.Outlined.Science),
    About(R.string.settings_category_about, R.string.settings_category_about_summary, Icons.Outlined.Info),

    /** Hidden until the About build-number gesture unlocks it. See `AndroidDeveloperOptions.kt`. */
    Developer(
        R.string.settings_category_developer,
        R.string.settings_category_developer_summary,
        Icons.Outlined.DeveloperMode,
    ),
}

internal data class LauncherBadge(
    val iconRes: Int,
    val name: String,
    val background: Color,
    val foreground: Color = SettingsText,
)

internal fun shouldEnableSettingsControllerNavigation(
    tvProfile: Boolean,
    controllerFamily: AndroidControllerFamily?,
    gamingHandheld: Boolean,
): Boolean = tvProfile || controllerFamily != null || gamingHandheld

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
    SettingsChoiceOption("fr_FR", "Français"),
    SettingsChoiceOption("es_ES", "Español (ES)"),
    SettingsChoiceOption("es_MX", "Español (MX)"),
    SettingsChoiceOption("it_IT", "Italiano"),
    SettingsChoiceOption("pt_PT", "Português (PT)"),
    SettingsChoiceOption("pt_BR", "Português (BR)"),
    SettingsChoiceOption("ru_RU", "Русский"),
    SettingsChoiceOption("pl_PL", "Polski"),
    SettingsChoiceOption("tr_TR", "Türkçe"),
    SettingsChoiceOption("ar_SA", "العربية"),
    SettingsChoiceOption("ja_JP", "日本語"),
    SettingsChoiceOption("ko_KR", "한국어"),
    SettingsChoiceOption("zh_CN", "简体中文"),
    SettingsChoiceOption("zh_TW", "繁體中文"),
    SettingsChoiceOption("th_TH", "ไทย"),
    SettingsChoiceOption("vi_VN", "Tiếng Việt"),
    SettingsChoiceOption("id_ID", "Bahasa Indonesia"),
    SettingsChoiceOption("cs_CZ", "Čeština"),
    SettingsChoiceOption("el_GR", "Ελληνικά"),
    SettingsChoiceOption("hu_HU", "Magyar"),
    SettingsChoiceOption("ro_RO", "Română"),
    SettingsChoiceOption("uk_UA", "Українська"),
    SettingsChoiceOption("nl_NL", "Nederlands"),
    SettingsChoiceOption("sv_SE", "Svenska"),
    SettingsChoiceOption("da_DK", "Dansk"),
    SettingsChoiceOption("fi_FI", "Suomi"),
    SettingsChoiceOption("no_NO", "Norsk"),
)

@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun SettingsScreen(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    tvProfile: Boolean,
    searchRequested: Boolean,
    searchQuery: String,
    backRequestToken: Int,
    onSearchQueryChange: (String) -> Unit,
    onDetailRouteChange: (Boolean) -> Unit,
) {
    var showSessionProxyWarning by remember { mutableStateOf(false) }
    var selectedCategory by remember { mutableStateOf<SettingsCategory?>(null) }
    val scrollState = rememberScrollState()
    val landingListState = rememberLazyListState()
    val detailListState = rememberLazyListState()
    val listState = if (selectedCategory == null && searchQuery.isBlank()) landingListState else detailListState
    val searchFocusRequester = remember { FocusRequester() }
    val detailFocusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    val focusManager = LocalFocusManager.current
    val controllerFamily = rememberPhysicalControllerFamily(enabled = true)
    val gamingHandheld = remember { isGamingHandheldDevice() }
    val controllerNavigationEnabled = shouldEnableSettingsControllerNavigation(
        tvProfile = tvProfile,
        controllerFamily = controllerFamily,
        gamingHandheld = gamingHandheld,
    )
    val showSearch = searchRequested || searchQuery.isNotBlank()
    val categories = remember(state.settings.developerOptionsUnlocked) {
        settingsCategories(state.settings.developerOptionsUnlocked)
    }
    val reduceMotion = LocalReduceMotion.current
    val platformBringIntoViewSpec = LocalBringIntoViewSpec.current
    val density = LocalDensity.current
    val focusTopClearancePx = with(density) { SettingsFocusTopClearance.toPx() }
    val focusBottomClearancePx = with(density) { SettingsFocusBottomClearance.toPx() }
    val settingsBringIntoViewSpec = remember(
        tvProfile,
        platformBringIntoViewSpec,
        focusTopClearancePx,
        focusBottomClearancePx,
    ) {
        if (tvProfile) {
            platformBringIntoViewSpec
        } else {
            object : BringIntoViewSpec {
                override fun calculateScrollDistance(
                    offset: Float,
                    size: Float,
                    containerSize: Float,
                ): Float = settingsFocusScrollDistance(
                    itemOffsetPx = offset,
                    itemSizePx = size,
                    containerSizePx = containerSize,
                    topClearancePx = focusTopClearancePx,
                    bottomClearancePx = focusBottomClearancePx,
                )
            }
        }
    }
    LaunchedEffect(searchRequested) {
        if (searchRequested) {
            delay(90)
            runCatching { searchFocusRequester.requestFocus() }
            keyboardController?.show()
        }
    }
    LaunchedEffect(categories) {
        if (selectedCategory != null &&
            selectedCategory !in settingsDetailCategories(state.settings.developerOptionsUnlocked)
        ) {
            selectedCategory = null
        }
    }
    LaunchedEffect(state.settingsRouteTarget) {
        val routeTarget = state.settingsRouteTarget ?: return@LaunchedEffect
        val routeCategory = when (routeTarget) {
            SettingsRouteTarget.Account -> SettingsCategory.Account
            SettingsRouteTarget.General -> SettingsCategory.General
            SettingsRouteTarget.Stream -> SettingsCategory.Stream
            SettingsRouteTarget.Interface -> SettingsCategory.Interface
        }
        if (selectedCategory != routeCategory || searchQuery.isNotBlank()) {
            onSearchQueryChange("")
            selectedCategory = routeCategory
        }
        viewModel.consumeSettingsRouteTarget(routeTarget)
    }
    BackHandler(enabled = selectedCategory != null) {
        selectedCategory = settingsCategoryParent(selectedCategory)
    }
    LaunchedEffect(selectedCategory, controllerNavigationEnabled) {
        val detailOpen = selectedCategory != null
        onDetailRouteChange(detailOpen)
        if (detailOpen && !tvProfile) {
            detailListState.scrollToItem(0)
        }
        if (detailOpen && controllerNavigationEnabled) {
            delay(90)
            runCatching { detailFocusRequester.requestFocus() }
        }
        if (tvProfile) {
            // Focus can scroll the first control into view while AnimatedContent is
            // still measuring the new route. Reset afterward so every TV detail
            // page opens with its header and remote Back hint fully visible.
            delay(30)
            scrollState.scrollTo(0)
        }
    }
    LaunchedEffect(backRequestToken) {
        if (backRequestToken > 0 && selectedCategory != null) {
            selectedCategory = settingsCategoryParent(selectedCategory)
        }
    }
    DisposableEffect(Unit) {
        onDispose { onDetailRouteChange(false) }
    }
    CompositionLocalProvider(
        LocalSettingsControllerNavigationEnabled provides controllerNavigationEnabled,
        LocalBringIntoViewSpec provides settingsBringIntoViewSpec,
    ) {
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
            SwipeToRefreshContainer(
                refreshing = state.settingsRefreshing,
                onRefresh = viewModel::refreshSettings,
                modifier = Modifier.fillMaxSize(),
                enabled = false,
            ) {
                Column(
                    Modifier
                        .fillMaxSize()
                        .onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) }
                        .verticalScroll(scrollState)
                        .padding(
                            start = 20.dp,
                            top = 20.dp,
                            end = 20.dp,
                            bottom = AppScrollEndSpacing,
                        ),
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
                    SettingsRouteContent(
                        targetState = selectedCategory,
                        reduceMotion = reduceMotion,
                    ) { category ->
                        SettingsBody(
                            state = state,
                            viewModel = viewModel,
                            tvProfile = tvProfile,
                            controllerFamily = controllerFamily,
                            searchQuery = searchQuery,
                            selectedCategory = category,
                            categories = categories,
                            detailFocusRequester = detailFocusRequester,
                            onSelectCategory = { selectedCategory = it },
                            onBack = { selectedCategory = settingsCategoryParent(selectedCategory) },
                            showSessionProxyWarning = { showSessionProxyWarning = true },
                        )
                    }
                }
            }
        } else {
            SwipeToRefreshContainer(
                refreshing = state.settingsRefreshing,
                onRefresh = viewModel::refreshSettings,
                modifier = Modifier.fillMaxSize(),
            ) {
                LazyColumn(
                    Modifier
                        .fillMaxSize(),
                    state = listState,
                    contentPadding = PaddingValues(
                        start = 14.dp,
                        top = 14.dp,
                        end = 14.dp,
                        bottom = AppScrollEndSpacing,
                    ),
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
                        SettingsRouteContent(
                            targetState = selectedCategory,
                            reduceMotion = reduceMotion,
                        ) { category ->
                            SettingsBody(
                                state = state,
                                viewModel = viewModel,
                                tvProfile = tvProfile,
                                controllerFamily = controllerFamily,
                                searchQuery = searchQuery,
                                selectedCategory = category,
                                categories = categories,
                                detailFocusRequester = detailFocusRequester,
                                onSelectCategory = { selectedCategory = it },
                                onBack = { selectedCategory = settingsCategoryParent(selectedCategory) },
                                showSessionProxyWarning = { showSessionProxyWarning = true },
                            )
                        }
                    }
                }
            }
        }
    }
}

/**
 * Slides only the incoming Settings route using layout placement.
 *
 * The outgoing route is discarded immediately, so two control-heavy Settings trees are never
 * measured or drawn together. Unlike the former alpha/translation implementation, this does not
 * create a page-sized RenderNode animation layer.
 */
@Composable
private fun SettingsRouteContent(
    targetState: SettingsCategory?,
    reduceMotion: Boolean,
    content: @Composable (SettingsCategory?) -> Unit,
) {
    var initialized by remember { mutableStateOf(false) }
    var previousDepth by remember { mutableStateOf(settingsRouteDepth(targetState)) }
    var direction by remember { mutableStateOf(1f) }
    val animateEntrance = initialized && !reduceMotion
    val progress = remember(targetState, reduceMotion) {
        Animatable(if (animateEntrance) 0f else 1f)
    }
    val slideDistancePx = with(LocalDensity.current) { 28.dp.toPx() }

    LaunchedEffect(targetState, reduceMotion) {
        val nextDepth = settingsRouteDepth(targetState)
        direction = if (nextDepth < previousDepth) -1f else 1f
        previousDepth = nextDepth
        if (!initialized) {
            initialized = true
        } else if (!reduceMotion) {
            progress.animateTo(
                targetValue = 1f,
                animationSpec = tween(
                    durationMillis = OpenNowMotion.DurationStandard,
                    easing = OpenNowMotion.EasingEmphasizedDecel,
                ),
            )
        }
    }

    Box(
        Modifier
            .fillMaxWidth()
            .offset {
                IntOffset(
                    x = (direction * slideDistancePx * (1f - progress.value)).roundToInt(),
                    y = 0,
                )
            },
    ) {
        content(targetState)
    }
}

@Composable
private fun SettingsBody(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    tvProfile: Boolean,
    controllerFamily: AndroidControllerFamily?,
    searchQuery: String,
    selectedCategory: SettingsCategory?,
    categories: List<SettingsCategory>,
    detailFocusRequester: FocusRequester,
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
                onSelectCategory = onSelectCategory,
                showSessionProxyWarning = showSessionProxyWarning,
            )
        }
        selectedCategory == null -> {
            SettingsCategoryLanding(
                state = state,
                viewModel = viewModel,
                categories = categories,
                onSelectCategory = onSelectCategory,
            )
        }
        else -> {
            Column(
                Modifier
                    .fillMaxWidth()
                    .lockedFocusGroup(),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                SettingsDetailHeader(
                    category = selectedCategory,
                    tvProfile = tvProfile,
                    controllerFamily = controllerFamily,
                    onBack = onBack,
                )
                Box(
                    Modifier
                        .fillMaxWidth()
                        .focusRequester(detailFocusRequester)
                        .focusGroup(),
                ) {
                    SettingsContent(
                        state = state,
                        viewModel = viewModel,
                        searchQuery = searchQuery,
                        selectedCategory = selectedCategory,
                        onSelectCategory = onSelectCategory,
                        showSessionProxyWarning = showSessionProxyWarning,
                    )
                }
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
    onSelectCategory: (SettingsCategory) -> Unit,
    showSessionProxyWarning: () -> Unit,
) {
    val settings = state.settings
    val context = LocalContext.current
    val gyroscopeAvailable = remember(context) { hasMobileGyroscope(context) }
    val deviceHasBattery = rememberDeviceHasBattery()
    val fallbackMembershipTier = state.authSession?.user?.membershipTier
    var pendingMicrophoneMode by remember { mutableStateOf<MicrophoneMode?>(null) }
    val microphonePermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        val requestedMode = pendingMicrophoneMode
        pendingMicrophoneMode = null
        if (granted && requestedMode != null) {
            viewModel.updateSettings(
                settings.copy(
                    stream = settings.stream.copy(microphoneMode = requestedMode),
                ),
            )
        } else if (!granted) {
            Toast.makeText(
                context,
                context.getString(R.string.settings_microphone_permission_denied),
                Toast.LENGTH_LONG,
            ).show()
        }
    }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
    CategorySettingsSection(selectedCategory, SettingsCategory.General, searchQuery, "App updates", "update", "updates", "disable update checking", "checking", "check", "download", "install", "apk") {
                if (state.androidUpdate.apkUpdatesAllowed) {
                    SettingSwitch(stringResource(R.string.settings_disable_update_checking), !settings.autoCheckForUpdates) { disabled ->
                        viewModel.updateSettings(settings.copy(autoCheckForUpdates = !disabled))
                    }
                }
                AndroidUpdatePanel(state = state, viewModel = viewModel)
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.General, searchQuery, stringResource(R.string.settings_section_language), "language", "locale", "english", "system default", "app language") {
                val appLocale = currentAndroidAppLocale(context)
                val systemDefaultLabel = stringResource(R.string.app_language_system_default)
                val languageOptions = listOf(
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_SYSTEM, systemDefaultLabel),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_ENGLISH, stringResource(R.string.app_language_english)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_ARABIC, stringResource(R.string.app_language_arabic)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_GERMAN, stringResource(R.string.app_language_german)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_SPANISH, stringResource(R.string.app_language_spanish)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_FRENCH, stringResource(R.string.app_language_french)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_JAPANESE, stringResource(R.string.app_language_japanese)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_KOREAN, stringResource(R.string.app_language_korean)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_DUTCH, stringResource(R.string.app_language_dutch)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_POLISH, stringResource(R.string.app_language_polish)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_PORTUGUESE, stringResource(R.string.app_language_portuguese)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_ROMANIAN, stringResource(R.string.app_language_romanian)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_RUSSIAN, stringResource(R.string.app_language_russian)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_TURKISH, stringResource(R.string.app_language_turkish)),
                    ChoiceMenuOption(ANDROID_APP_LANGUAGE_SIMPLIFIED_CHINESE, stringResource(R.string.app_language_simplified_chinese)),
                )
                ChoiceMenuRow(
                    label = stringResource(R.string.settings_app_language),
                    options = languageOptions,
                    selectedLabel = if (appLocale.selectedLanguageTag.isBlank()) {
                        "$systemDefaultLabel (${appLocale.effectiveLanguageTag.ifBlank { "unknown" }})"
                    } else {
                        languageOptions.firstOrNull { it.value == appLocale.selectedLanguageTag }?.label
                            ?: appLocale.selectedLanguageTag
                    },
                ) { languageTag ->
                    setAndroidAppLanguage(context, languageTag)
                }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Advanced, searchQuery, stringResource(R.string.settings_nerd_mode), "advanced", "advanced options", "nerd", "experimental", "diagnostics") {
                AdvancedOptionsSettings(settings = settings, viewModel = viewModel)
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Stream, searchQuery, stringResource(R.string.settings_section_stream_quality), "stream", "quality", "preset", "data saver", "low", "medium", "high", "custom", "resolution", "aspect ratio", "fps", "bitrate") {
                ChoiceMenuRow(
                    label = stringResource(R.string.settings_stream_preset),
                    options = StreamPreset.entries.map { preset ->
                        ChoiceMenuOption(
                            value = preset.name,
                            label = streamPresetLabel(preset),
                        )
                    },
                    selectedLabel = streamPresetLabel(settings.streamPreset),
                ) { value ->
                    viewModel.applyStreamPreset(StreamPreset.valueOf(value))
                }
                state.recommendedStreamSettings?.let { recommended ->
                    Text(
                        stringResource(
                            R.string.settings_detected_recommendation,
                            recommended.recommendationSummary(),
                        ),
                        color = SettingsTextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                val recommendationOverrides = settings.stream.performanceOverridesComparedTo(
                    recommended = state.recommendedStreamSettings,
                    report = state.codecReport,
                )
                val performanceWarningReasons = settings.stream.lowPowerPerformanceWarningReasons(state.codecReport)
                val performanceWarnings = recommendationOverrides.ifEmpty { performanceWarningReasons }
                if (performanceWarnings.isNotEmpty()) {
                    DeviceStreamRecommendationWarning(
                        reasons = performanceWarnings,
                        recommended = state.recommendedStreamSettings,
                    )
                }
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
                SettingSwitch(
                    label = stringResource(R.string.settings_stretch_stream_to_fit),
                    checked = settings.stretchStreamToFit,
                ) { enabled ->
                    viewModel.updateSettings(
                        settings.copy(
                            legacyCropStreamToFill = false,
                            stretchStreamToFit = enabled,
                        ),
                    )
                }
                val maxFps = maxStreamFpsFor(state.subscriptionInfo, fallbackMembershipTier)
                NumberSlider(
                    label = stringResource(R.string.settings_fps),
                    value = settings.stream.fps.coerceAtMost(maxFps).toFloat(),
                    min = 30f,
                    max = maxFps.toFloat(),
                    step = 30f,
                    unit = "FPS",
                ) {
                    val fps = it.roundToInt().coerceIn(30, maxFps)
                    viewModel.updateStreamSettings { s -> s.copy(fps = fps) }
                }
                NumberSlider(
                    label = stringResource(R.string.settings_bitrate),
                    value = settings.stream.maxBitrateMbps.toFloat(),
                    min = 1f,
                    max = 150f,
                    step = 1f,
                    descriptionProvider = { mbps -> streamBitrateUsageEstimate(mbps) },
                ) {
                    viewModel.updateStreamSettings { s -> s.copy(maxBitrateMbps = it.roundToInt()) }
                }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Stream, searchQuery, stringResource(R.string.settings_section_stream_video), "stream", "video", "codec", "color", "hdr", "sharpening", "native streamer", "low latency", "native decoder", "decoder") {
                val comingSoonLabel = stringResource(R.string.option_coming_soon)
                val unavailableLabel = stringResource(R.string.common_unavailable)
                val h264H265OnlyLabel = stringResource(R.string.settings_av1_ten_bit_badge)
                val settingsAvailableStream = settings.stream.withAndroidSettingsAvailability()
                val codecChoices = androidCodecChoicePresentation(
                    stream = settings.stream,
                    codecReport = state.codecReport,
                    comingSoonLabel = comingSoonLabel,
                    unavailableLabel = unavailableLabel,
                )
                ChoiceMenuRow(
                    label = stringResource(R.string.settings_codec),
                    options = codecChoices.options,
                    selectedLabel = codecChoices.selectedLabel,
                    description = stringResource(R.string.settings_codec_desc),
                ) { value ->
                    val selectedCodec = VideoCodec.valueOf(value)
                    val downgradedTenBit = selectedCodec == VideoCodec.AV1 &&
                        settings.stream.usesTenBitStreamProfile()
                    viewModel.updateStreamSettings { s ->
                        s.copy(codec = selectedCodec).withCodecColorCompatibility()
                    }
                    if (downgradedTenBit) {
                        Toast.makeText(
                            context,
                            context.getString(R.string.settings_av1_ten_bit_downgraded),
                            Toast.LENGTH_LONG,
                        ).show()
                    }
                }
                val effectiveColorQuality = settingsAvailableStream.withCodecColorCompatibility().colorQuality
                ChoiceMenuRow(
                    label = stringResource(R.string.settings_color),
                    options = ColorQuality.entries.map { quality ->
                        val available = quality.availableForCodec(settingsAvailableStream.codec)
                        ChoiceMenuOption(
                            value = quality.name,
                            label = quality.label,
                            enabled = available,
                            badge = when {
                                available -> null
                                settingsAvailableStream.codec == VideoCodec.AV1 && quality == ColorQuality.TenBit420 -> h264H265OnlyLabel
                                else -> comingSoonLabel
                            },
                        )
                    },
                    selectedLabel = if (effectiveColorQuality == settings.stream.colorQuality) {
                        settings.stream.colorQuality.label
                    } else {
                        "${settings.stream.colorQuality.label} -> ${effectiveColorQuality.label}"
                    },
                    description = stringResource(R.string.settings_color_desc),
                ) { value ->
                    viewModel.updateStreamSettings { s ->
                        s.copy(colorQuality = ColorQuality.valueOf(value)).withCodecColorCompatibility()
                    }
                }
                if (settingsAvailableStream.codec == VideoCodec.AV1) {
                    Text(
                        stringResource(R.string.settings_av1_ten_bit_hint),
                        color = SettingsTextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                val hdrDeviceAvailable by produceState(false, settingsAvailableStream.codec,
                    settingsAvailableStream.resolution, settingsAvailableStream.fps) {
                    value = withContext(Dispatchers.Default) {
                        settingsAvailableStream.copy(hdrEnabled = true).withHdrDeviceSupport(context).hdrEnabled
                    }
                }
                val hdrAvailable = hasHdrStreamingPlan(state.subscriptionInfo, fallbackMembershipTier) && hdrDeviceAvailable
                SettingSwitch(
                    label = stringResource(R.string.settings_hdr),
                    checked = settings.stream.hdrEnabled && hdrAvailable,
                    enabled = hdrAvailable,
                    description = stringResource(R.string.settings_hdr_desc),
                ) { enabled ->
                    viewModel.updateStreamSettings { s ->
                        s.copy(
                            hdrEnabled = enabled,
                            colorQuality = if (enabled && !s.colorQuality.name.startsWith("TenBit")) ColorQuality.TenBit420 else s.colorQuality,
                        ).withCodecColorCompatibility()
                    }
                }
                if (!hdrDeviceAvailable) {
                    Text(
                        if (state.androidTvProfile) {
                            stringResource(R.string.settings_hdr_android_tv_compatibility_hint)
                        } else {
                            stringResource(R.string.settings_hdr_android_handheld_hint)
                        },
                        color = SettingsTextMuted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                SettingSwitch(
                    label = stringResource(R.string.stream_panel_sharpening),
                    checked = settings.stream.streamSharpeningEnabled,
                    description = stringResource(R.string.settings_stream_sharpening_desc),
                ) {
                    viewModel.updateStreamSettings { s -> s.copy(streamSharpeningEnabled = it) }
                }
                if (settings.stream.streamSharpeningEnabled) {
                    NumberSlider(
                        label = stringResource(R.string.stream_panel_sharpening_amount),
                        value = settings.stream.streamSharpeningAmount,
                        min = 0f,
                        max = 1f,
                        step = 0.05f,
                        description = stringResource(R.string.settings_stream_sharpening_amount_desc),
                    ) {
                        viewModel.updateStreamSettings { s -> s.copy(streamSharpeningAmount = it) }
                    }
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_native_streamer),
                    checked = settings.nativeLowLatencyDecoder,
                    description = stringResource(R.string.settings_native_streamer_desc),
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(nativeLowLatencyDecoder = enabled))
                }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Stream, searchQuery, stringResource(R.string.settings_section_stream_connection), "stream", "connection", "network", "region", "session proxy", "proxy") {
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
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Input, searchQuery, stringResource(R.string.settings_section_audio_keyboard), "input", "microphone", "mic", "voice", "audio", "keyboard", "shortcut", "layout", "language", "clipboard", "paste") {
                SettingSwitch(
                    label = stringResource(R.string.settings_microphone),
                    checked = settings.stream.microphoneMode != MicrophoneMode.Disabled,
                    description = stringResource(R.string.settings_microphone_desc),
                ) { enabled ->
                    if (!enabled) {
                        viewModel.updateSettings(
                            settings.copy(
                                stream = settings.stream.copy(microphoneMode = MicrophoneMode.Disabled),
                            ),
                        )
                    } else if (
                        ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) ==
                        PackageManager.PERMISSION_GRANTED
                    ) {
                        viewModel.updateSettings(
                            settings.copy(
                                stream = settings.stream.copy(microphoneMode = MicrophoneMode.VoiceActivity),
                            ),
                        )
                    } else {
                        pendingMicrophoneMode = MicrophoneMode.VoiceActivity
                        microphonePermissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
                    }
                }
                ChoiceOptionRow(stringResource(R.string.settings_keyboard_layout), keyboardLayoutOptions, settings.stream.keyboardLayout) {
                    viewModel.updateStreamSettings { s -> s.copy(keyboardLayout = it) }
                }
                ChoiceOptionRow(stringResource(R.string.settings_game_language), gameLanguageOptions, settings.stream.gameLanguage) {
                    viewModel.updateStreamSettings { s -> s.copy(gameLanguage = it) }
                }
                SettingSwitch(stringResource(R.string.settings_clipboard_paste), settings.clipboardPaste) { enabled -> viewModel.updateSettings(settings.copy(clipboardPaste = enabled)) }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Input, searchQuery, stringResource(R.string.settings_section_pointer_input), "input", "pointer", "mouse", "lock", "grab", "capture", "fullscreen", "sensitivity", "acceleration", "scroll", "controller mouse", "mode", "native touch", "tap", "stability", "finger", "direct click") {
                SettingSwitch(
                    label = stringResource(R.string.settings_mouse_lock),
                    checked = settings.externalMousePointerLock,
                    description = stringResource(R.string.settings_mouse_lock_desc),
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(externalMousePointerLock = enabled))
                }
                NumberSlider("Mouse sensitivity", settings.stream.mouseSensitivity, 0.25f, 3f, 0.05f, valueFormatter = { "%.2fx".format(it) }) {
                    viewModel.updateStreamSettings { s -> s.copy(mouseSensitivity = it) }
                }
                NumberSlider("Mouse acceleration", settings.stream.mouseAcceleration.toFloat(), 1f, 150f, 1f) {
                    viewModel.updateStreamSettings { s -> s.copy(mouseAcceleration = it.roundToInt()) }
                }
                val scrollHint = when {
                    settings.stream.mouseScrollSensitivity <= 20 -> "Very fast"
                    settings.stream.mouseScrollSensitivity <= 40 -> "Standard"
                    settings.stream.mouseScrollSensitivity <= 60 -> "Precise"
                    else -> "Slow"
                }
                NumberSlider(
                    label = "Mouse scroll sensitivity",
                    value = settings.stream.mouseScrollSensitivity.toFloat(),
                    min = 10f,
                    max = 100f,
                    step = 5f,
                    unit = " ($scrollHint)",
                ) { value ->
                    viewModel.updateStreamSettings { s -> s.copy(mouseScrollSensitivity = value.toInt()) }
                }
                SettingSwitch(
                    label = stringResource(R.string.stream_panel_mouse_mode),
                    checked = settings.controllerMouseEmulation,
                    description = "Toggle in Stream Controls per session. Left stick moves the cursor, right stick scrolls, A button clicks, B button right-clicks.",
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(controllerMouseEmulation = enabled))
                }
                if (!state.androidTvProfile) {
                    // Sends fingers to the PC as a real touchscreen, so games with a touch mode switch
                    // to it themselves. Auto is limited to catalog variants that advertise touch.
                    val effectiveNativeTouchMode = settings.androidTouch.effectiveNativeTouchMode()
                    ChoiceMenuRow(
                        label = "Native touch",
                        options = NativeTouchMode.entries.map { mode ->
                            ChoiceMenuOption(value = mode.name, label = nativeTouchModeLabel(mode))
                        },
                        selectedLabel = nativeTouchModeLabel(effectiveNativeTouchMode),
                    ) { value ->
                        val mode = NativeTouchMode.entries.firstOrNull { it.name == value } ?: NativeTouchMode.Off
                        viewModel.updateSettings(
                            settings.copy(androidTouch = settings.androidTouch.withNativeTouchMode(mode)),
                        )
                    }
                    if (
                        effectiveNativeTouchMode == NativeTouchMode.Auto &&
                        settings.stream.requiresNativeDesktopCloudMatchMode()
                    ) {
                        Text(
                            text = stringResource(R.string.settings_native_touch_high_performance_hint),
                            color = SettingsTextMuted,
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
                SettingSwitch(stringResource(R.string.stream_panel_finger_mouse), settings.androidTouch.mousePad) { enabled -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(mousePad = enabled))) }
                if (settings.androidTouch.mousePad) {
                    Box(Modifier.padding(start = 24.dp)) {
                        SettingSwitch(stringResource(R.string.stream_panel_direct_click), settings.androidTouch.mouseDirectClick) { enabled -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(mouseDirectClick = enabled))) }
                    }
                }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Input, searchQuery, stringResource(R.string.settings_section_controller_touch), "input", "rumble", "touch", "controller", "style", "skin", "theme", "colour", "color", "labels", "layout", "scale", "size", "opacity", "edge", "padding", "offset", "horizontal", "vertical", "controls", "visible", "hide", "programmable", "extra", "accessibility", "guide", "home", "stick", "joystick", "analog", "dynamic", "dead zone", "button") {
                SettingSwitch(
                    label = stringResource(R.string.stream_panel_vibration),
                    checked = settings.vibrationEnabled,
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(vibrationEnabled = enabled))
                }
                if (settings.vibrationEnabled) {
                    val hapticsOutputOptions = listOf(
                        SettingsChoiceOption(
                            HapticsOutputPreference.Auto.name,
                            stringResource(R.string.settings_haptics_output_auto),
                        ),
                        SettingsChoiceOption(
                            HapticsOutputPreference.Controller.name,
                            stringResource(R.string.settings_haptics_output_controller),
                        ),
                        SettingsChoiceOption(
                            HapticsOutputPreference.Device.name,
                            stringResource(R.string.settings_haptics_output_device),
                        ),
                    )
                    ChoiceOptionRow(
                        stringResource(R.string.settings_haptics_output),
                        hapticsOutputOptions,
                        settings.hapticsOutput.name,
                        description = stringResource(R.string.settings_haptics_output_desc),
                    ) { name ->
                        viewModel.updateSettings(
                            settings.copy(hapticsOutput = HapticsOutputPreference.valueOf(name)),
                        )
                    }
                }
                SettingSwitch(stringResource(R.string.stream_touch_controls_title), settings.androidTouch.enabled) { enabled -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(enabled = enabled))) }
                val touchStyleOptions = TouchControllerStyle.entries.map { style ->
                    SettingsChoiceOption(style.name, touchControllerStyleLabel(style))
                }
                ChoiceOptionRow(
                    stringResource(R.string.settings_touch_skin),
                    touchStyleOptions,
                    settings.androidTouch.touchControllerStyle.name,
                    description = stringResource(R.string.settings_touch_skin_desc),
                ) { styleName ->
                    val style = TouchControllerStyle.valueOf(styleName)
                    viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(touchControllerStyle = style)))
                }
                // The skins differ by shape now, which a list of names cannot show.
                TouchControllerSkinPreview(
                    style = settings.androidTouch.touchControllerStyle,
                    tint = settings.androidTouch.touchSkinTint,
                    opacity = settings.androidTouch.opacity,
                    showLabels = settings.androidTouch.touchButtonLabels,
                    modifier = Modifier.padding(vertical = 8.dp),
                )
                val touchTintOptions = TOUCH_SKIN_TINTS.map { SettingsChoiceOption(it.id, it.label) }
                ChoiceOptionRow(
                    stringResource(R.string.settings_touch_skin_tint),
                    touchTintOptions,
                    touchSkinTintId(settings.androidTouch.touchSkinTint),
                    description = stringResource(R.string.settings_touch_skin_tint_desc),
                ) { tintId ->
                    viewModel.updateSettings(
                        settings.copy(
                            androidTouch = settings.androidTouch.copy(touchSkinTint = touchSkinTintForId(tintId)),
                        ),
                    )
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_touch_button_labels),
                    checked = settings.androidTouch.touchButtonLabels,
                ) { enabled ->
                    viewModel.updateSettings(
                        settings.copy(androidTouch = settings.androidTouch.copy(touchButtonLabels = enabled)),
                    )
                }
                Text(
                    text = stringResource(R.string.settings_touch_visible_controls),
                    color = SettingsText,
                    style = MaterialTheme.typography.titleSmall,
                )
                Text(
                    text = stringResource(R.string.settings_touch_visible_controls_desc),
                    color = SettingsTextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
                TouchControlGroup.entries.forEach { group ->
                    SettingSwitch(
                        label = stringResource(touchControlGroupLabelRes(group)),
                        checked = settings.androidTouch.isControlVisible(group),
                    ) { visible ->
                        viewModel.updateSettings(
                            settings.copy(androidTouch = settings.androidTouch.withControlVisible(group, visible)),
                        )
                    }
                }
                Text(
                    text = stringResource(R.string.settings_touch_extra_buttons),
                    color = SettingsText,
                    style = MaterialTheme.typography.titleSmall,
                )
                Text(
                    text = stringResource(R.string.settings_touch_extra_buttons_desc),
                    color = SettingsTextMuted,
                    style = MaterialTheme.typography.bodySmall,
                )
                val extraButtonOptions = TouchExtraButtonAction.entries.map { action ->
                    SettingsChoiceOption(action.name, touchExtraButtonActionLabel(action))
                }
                repeat(TOUCH_EXTRA_BUTTON_COUNT) { index ->
                    ChoiceOptionRow(
                        label = stringResource(R.string.settings_touch_extra_button, index + 1),
                        options = extraButtonOptions,
                        selectedValue = settings.androidTouch.extraButtonAction(index).name,
                    ) { actionName ->
                        val action = TouchExtraButtonAction.valueOf(actionName)
                        viewModel.updateSettings(
                            settings.copy(androidTouch = settings.androidTouch.withExtraButtonAction(index, action)),
                        )
                    }
                }
                NumberSlider(
                    stringResource(R.string.settings_touch_extra_button_size),
                    settings.androidTouch.extraButtonScale,
                    0.6f,
                    1.6f,
                    0.05f,
                ) { value ->
                    viewModel.updateSettings(
                        settings.copy(androidTouch = settings.androidTouch.copy(extraButtonScale = value)),
                    )
                }
                val touchAimOptions = listOf(
                    SettingsChoiceOption(TouchAimMode.LockJoystick.name, stringResource(R.string.stream_joysticks_lock_joystick)),
                    SettingsChoiceOption(TouchAimMode.LockZone.name, stringResource(R.string.stream_joysticks_lock_zone)),
                )
                ChoiceOptionRow(stringResource(R.string.stream_joysticks_aim_mode), touchAimOptions, settings.androidTouch.aimMode.name) { modeName ->
                    val mode = TouchAimMode.valueOf(modeName)
                    viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(aimMode = mode)))
                }
                if (settings.androidTouch.aimMode == TouchAimMode.LockZone) {
                    NumberSlider(
                        stringResource(R.string.stream_joysticks_aim_zone_scale),
                        settings.androidTouch.aimZoneScale,
                        0.5f,
                        1.5f,
                        0.05f,
                    ) { value ->
                        viewModel.updateSettings(
                            settings.copy(androidTouch = settings.androidTouch.copy(aimZoneScale = value)),
                        )
                    }
                    NumberSlider(
                        stringResource(R.string.stream_joysticks_aim_zone_sensitivity),
                        settings.androidTouch.aimZoneSensitivity,
                        0.25f,
                        3f,
                        0.05f,
                    ) { value ->
                        viewModel.updateSettings(
                            settings.copy(androidTouch = settings.androidTouch.copy(aimZoneSensitivity = value)),
                        )
                    }
                }
                val joystickModeOptions = listOf(
                    SettingsChoiceOption(TouchJoystickMode.Fixed.name, stringResource(R.string.stream_panel_joystick_fixed)),
                    SettingsChoiceOption(TouchJoystickMode.Dynamic.name, stringResource(R.string.stream_panel_joystick_dynamic)),
                )
                ChoiceOptionRow("Touch joystick", joystickModeOptions, settings.androidTouch.joystickMode.name) { modeName ->
                    val mode = TouchJoystickMode.valueOf(modeName)
                    viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(joystickMode = mode)))
                }
                NumberSlider("Joystick dead zone", settings.androidTouch.joystickDeadZone, 0f, 0.3f, 0.01f) { value ->
                    viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(joystickDeadZone = value)))
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_touch_gyro),
                    checked = settings.androidTouch.gyroscopeEnabled && gyroscopeAvailable,
                    enabled = gyroscopeAvailable,
                    description = stringResource(
                        if (gyroscopeAvailable) R.string.settings_touch_gyro_desc
                        else R.string.settings_touch_gyro_unavailable,
                    ),
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(gyroscopeEnabled = enabled)))
                }
                if (settings.androidTouch.gyroscopeEnabled && gyroscopeAvailable) {
                    NumberSlider(stringResource(R.string.settings_touch_gyro_sensitivity), settings.androidTouch.gyroscopeSensitivity, 0.25f, 3f, 0.05f) { value ->
                        viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(gyroscopeSensitivity = value)))
                    }
                    NumberSlider(stringResource(R.string.settings_touch_gyro_dead_zone), settings.androidTouch.gyroscopeDeadZone, 0f, 0.2f, 0.005f) { value ->
                        viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(gyroscopeDeadZone = value)))
                    }
                    NumberSlider(stringResource(R.string.settings_touch_gyro_smoothing), settings.androidTouch.gyroscopeSmoothing, 0f, 0.9f, 0.05f) { value ->
                        viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(gyroscopeSmoothing = value)))
                    }
                    SettingSwitch(
                        stringResource(R.string.settings_touch_gyro_invert_horizontal),
                        settings.androidTouch.gyroscopeInvertHorizontal,
                    ) { enabled ->
                        viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(gyroscopeInvertHorizontal = enabled)))
                    }
                    SettingSwitch(
                        stringResource(R.string.settings_touch_gyro_invert_vertical),
                        settings.androidTouch.gyroscopeInvertVertical,
                    ) { enabled ->
                        viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(gyroscopeInvertVertical = enabled)))
                    }
                }
                NumberSlider("Touch layout scale", settings.androidTouch.scale, 0.6f, 1.4f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(scale = value))) }
                NumberSlider("Touch button size", settings.androidTouch.buttonScale, 0.65f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(buttonScale = value))) }
                NumberSlider("Touch stick size", settings.androidTouch.stickScale, 0.65f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(stickScale = value))) }
                NumberSlider(stringResource(R.string.settings_touch_face_size), settings.androidTouch.faceButtonScale, 0.6f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(faceButtonScale = value))) }
                NumberSlider(stringResource(R.string.settings_touch_dpad_size), settings.androidTouch.dpadScale, 0.6f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(dpadScale = value))) }
                NumberSlider(stringResource(R.string.settings_touch_shoulders_size), settings.androidTouch.shoulderButtonScale, 0.6f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(shoulderButtonScale = value))) }
                NumberSlider(stringResource(R.string.settings_touch_center_size), settings.androidTouch.centerButtonScale, 0.6f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(centerButtonScale = value))) }
                NumberSlider(stringResource(R.string.settings_touch_left_stick_size), settings.androidTouch.leftStickScale, 0.6f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(leftStickScale = value))) }
                NumberSlider(stringResource(R.string.settings_touch_right_stick_size), settings.androidTouch.rightStickScale, 0.6f, 1.5f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(rightStickScale = value))) }
                NumberSlider(stringResource(R.string.settings_touch_stick_knob_size), settings.androidTouch.stickKnobScale, 0.28f, 0.72f, 0.02f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(stickKnobScale = value))) }
                NumberSlider("Touch opacity", settings.androidTouch.opacity, 0f, 1f, 0.05f) { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(opacity = value))) }
                NumberSlider("Touch edge padding", settings.androidTouch.edgePaddingDp, 0f, 72f, 1f, unit = "dp") { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(edgePaddingDp = value))) }
                NumberSlider("Touch bottom padding", settings.androidTouch.bottomPaddingDp, 0f, 120f, 1f, unit = "dp") { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(bottomPaddingDp = value))) }
                NumberSlider("Left controls horizontal offset", settings.androidTouch.leftOffsetXDp, -220f, 220f, 2f, unit = "dp") { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(leftOffsetXDp = value))) }
                NumberSlider("Left controls vertical offset", settings.androidTouch.leftOffsetYDp, -160f, 160f, 2f, unit = "dp") { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(leftOffsetYDp = value))) }
                NumberSlider("Right controls horizontal offset", settings.androidTouch.rightOffsetXDp, -220f, 220f, 2f, unit = "dp") { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(rightOffsetXDp = value))) }
                NumberSlider("Right controls vertical offset", settings.androidTouch.rightOffsetYDp, -160f, 160f, 2f, unit = "dp") { value -> viewModel.updateSettings(settings.copy(androidTouch = settings.androidTouch.copy(rightOffsetYDp = value))) }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Interface, searchQuery, stringResource(R.string.settings_section_appearance), "interface", "ui", "appearance", "dynamic color", "system colors", "accent", "expressive", "border", "effects", "bonanza", "cinema", "catalog", "background", "wallpaper", "image", "custom", "tv", "safe area", "screen padding", "overscan", "useless mascot", "screensaver", "inactivity") {
                val accentOptions = selectableUiAccents().map { it to uiAccentLabel(it) }
                SettingSwitch(stringResource(R.string.settings_dynamic_color), settings.dynamicColor) { viewModel.updateSettings(settings.copy(dynamicColor = it)) }
                ChoiceRow(
                    label = stringResource(R.string.settings_accent),
                    options = accentOptions.map { it.second },
                    selected = accentOptions.firstOrNull { it.first == settings.uiAccent }?.second ?: accentOptions.first().second,
                    activeOutlineColor = LocalActiveSelectionColor.current.takeIf { LocalActiveSelectionEnabled.current },
                    activeOutlineSecondaryColor = LocalActiveSelectionSecondaryColor.current.takeIf { LocalActiveSelectionEnabled.current },
                ) { label ->
                    accentOptions.firstOrNull { it.second == label }?.first?.let { accent ->
                        viewModel.updateSettings(settings.copy(uiAccent = accent))
                    }
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_live_selected_outlines),
                    checked = settings.liveSelectedOutlines,
                    description = stringResource(R.string.settings_live_selected_outlines_desc),
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(liveSelectedOutlines = enabled))
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_absolute_cinema_effects),
                    checked = settings.absoluteCinemaEffects,
                    description = stringResource(R.string.settings_absolute_cinema_effects_desc),
                ) { enabled ->
                    viewModel.updateSettings(
                        settings.copy(
                            absoluteCinemaEffects = enabled,
                            absoluteCinemaEverywhere = settings.absoluteCinemaEverywhere && enabled,
                        ),
                    )
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_im_crazy),
                    checked = settings.absoluteCinemaEverywhere,
                    enabled = settings.absoluteCinemaEffects,
                    description = stringResource(R.string.settings_im_crazy_desc),
                    indentLevel = 1,
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(absoluteCinemaEverywhere = enabled))
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_useless_mascot),
                    checked = settings.uselessMascotEnabled,
                    description = stringResource(R.string.settings_useless_mascot_desc),
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(uselessMascotEnabled = enabled))
                }
                if (settings.uselessMascotEnabled) {
                    NumberSlider(
                        label = stringResource(R.string.settings_useless_mascot_delay),
                        value = settings.uselessMascotDelaySeconds.toFloat(),
                        min = 5f,
                        max = 300f,
                        step = 5f,
                        unit = "s",
                    ) { seconds ->
                        viewModel.updateSettings(settings.copy(uselessMascotDelaySeconds = seconds.toInt()))
                    }
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_expressive_ui),
                    checked = settings.expressiveUi,
                ) {
                    viewModel.updateSettings(settings.copy(expressiveUi = it))
                }
                if (BuildConfig.LOCAL_APP_LAUNCHER_SUPPORTED) {
                    SettingSwitch(
                        label = stringResource(R.string.settings_local_apps),
                        checked = settings.localAppsEnabled,
                        description = stringResource(R.string.settings_local_apps_desc),
                    ) { enabled ->
                        viewModel.updateSettings(settings.copy(localAppsEnabled = enabled))
                    }
                    if (settings.localAppsEnabled) {
                        DefaultLauncherSetting()
                    }
                }
                NumberSlider(stringResource(R.string.settings_tv_safe_area), settings.tvSafeAreaPaddingDp, 0f, 72f, 2f, unit = "dp") { value ->
                    viewModel.updateSettings(settings.copy(tvSafeAreaPaddingDp = value))
                }
                CatalogBackgroundSettings(settings = settings, viewModel = viewModel)
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Interface, searchQuery, stringResource(R.string.settings_section_library_navigation), "interface", "launch page", "default page", "store", "library", "compact", "cards", "titles", "favorites", "favourites", "save", "icon", "game card size", "server selector", "hero", "banner", "featured", "landscape", "new games") {
                SettingSwitch(
                    label = stringResource(R.string.settings_landscape_new_games),
                    checked = settings.landscapeNewGamesHero,
                    description = stringResource(R.string.settings_landscape_new_games_desc),
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(landscapeNewGamesHero = enabled))
                }
                val launchPageOptions = AppLaunchPage.entries.map { page -> page to appLaunchPageLabel(page) }
                ChoiceRow(
                    stringResource(R.string.settings_launch_page),
                    launchPageOptions.map { it.second },
                    launchPageOptions.firstOrNull { it.first == settings.launchPage }?.second
                        ?: launchPageOptions.first().second,
                ) { label ->
                    launchPageOptions.firstOrNull { it.second == label }?.first?.let { page ->
                        viewModel.updateSettings(settings.copy(launchPage = page))
                    }
                }
                SettingSwitch(stringResource(R.string.settings_compact_cards), settings.compactGameCards) { viewModel.updateSettings(settings.copy(compactGameCards = it)) }
                SettingSwitch(stringResource(R.string.settings_show_card_titles), settings.showCardTitles) { viewModel.updateSettings(settings.copy(showCardTitles = it)) }
                SettingSwitch(
                    label = stringResource(R.string.settings_show_favorite_icon),
                    checked = settings.showFavoriteIconOnGameCards,
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(showFavoriteIconOnGameCards = enabled))
                }
                NumberSlider(
                    label = stringResource(R.string.settings_card_size),
                    value = settings.posterSizeScale,
                    min = MIN_GAME_CARD_SCALE,
                    max = MAX_GAME_CARD_SCALE,
                    step = 0.05f,
                    description = stringResource(R.string.settings_card_size_desc),
                ) { value ->
                    viewModel.updateSettings(settings.copy(posterSizeScale = value))
                }
                SettingSwitch(stringResource(R.string.settings_hide_server_selector), settings.hideServerSelector) { viewModel.updateSettings(settings.copy(hideServerSelector = it)) }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Interface, searchQuery, stringResource(R.string.settings_section_status_bar), "interface", "stats", "status bar", "position", "fps", "ping", "bitrate", "keyboard", "button") {
                SettingSwitch(stringResource(R.string.settings_show_stats), settings.showStatsOnLaunch) { viewModel.updateSettings(settings.copy(showStatsOnLaunch = it)) }
                SettingSwitch(
                    label = stringResource(R.string.settings_stream_keyboard_button),
                    checked = !settings.hideStreamButtons,
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(hideStreamButtons = !enabled))
                }
                ChoiceRow("Status bar appearance", StreamStatsStyle.entries.map { it.label }, settings.streamStatsStyle.label) { label ->
                    StreamStatsStyle.entries.firstOrNull { it.label == label }?.let { style ->
                        viewModel.updateSettings(settings.copy(streamStatsStyle = style))
                    }
                }
                ChoiceRow(stringResource(R.string.settings_stats_position), StreamStatsPosition.entries.map { it.label }, settings.streamStatsPosition.label) { label ->
                    StreamStatsPosition.entries.firstOrNull { it.label == label }?.let { position ->
                        viewModel.updateSettings(settings.copy(streamStatsPosition = position))
                    }
                }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Interface, searchQuery, stringResource(R.string.settings_section_sounds_sessions), "interface", "controller", "sounds", "button", "tone", "session counter", "session report", "quality summary", "intro", "music", "queue") {
                SettingSwitch(
                    label = stringResource(R.string.settings_button_press_tones),
                    checked = settings.controllerUiSounds,
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(controllerUiSounds = enabled))
                }
                SettingSwitch(stringResource(R.string.settings_session_counter), settings.sessionCounterEnabled) { viewModel.updateSettings(settings.copy(sessionCounterEnabled = it)) }
                SettingSwitch(
                    label = stringResource(R.string.settings_show_session_report),
                    checked = settings.showSessionReportAfterStream,
                    description = stringResource(R.string.settings_show_session_report_desc),
                ) { enabled ->
                    viewModel.updateSettings(settings.copy(showSessionReportAfterStream = enabled))
                }
                SettingSwitch(stringResource(R.string.settings_stream_intro_music), settings.streamIntroMusic) { enabled ->
                    viewModel.updateSettings(settings.copy(streamIntroMusic = enabled))
                }
                if (settings.streamIntroMusic) {
                    val introStartOptions = IntroMusicStartMode.entries.map { mode ->
                        mode to introMusicStartModeLabel(mode)
                    }
                    ChoiceRow(
                        stringResource(R.string.settings_stream_intro_music_start),
                        introStartOptions.map { it.second },
                        introStartOptions.firstOrNull { it.first == settings.streamIntroStartMode }?.second
                            ?: introStartOptions.first().second,
                    ) { label ->
                        introStartOptions.firstOrNull { it.second == label }?.first?.let { mode ->
                            viewModel.updateSettings(settings.copy(streamIntroStartMode = mode))
                        }
                    }
                }
                SettingSwitch(stringResource(R.string.settings_queue_ready_music), settings.queueReadyMusic) { enabled ->
                    viewModel.updateSettings(settings.copy(queueReadyMusic = enabled))
                }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.General, searchQuery, stringResource(R.string.settings_section_setup), "setup", "intro", "onboarding", "welcome", "first run", "walkthrough", "tour", "getting started") {
                ControlActionRow(
                    label = stringResource(R.string.settings_run_setup_again),
                    actionLabel = stringResource(R.string.action_open),
                    value = stringResource(R.string.settings_run_setup_again_desc),
                    onClick = { viewModel.updateSettings(settings.restartingSetupFlow()) },
                )
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.General, searchQuery, "App Data", "app data", "data", "cache", "clear", "reset", "settings", "tutorial", "guide", "wipe", "relaunch", "fresh install") {
                AppDataSettingsPanel(viewModel = viewModel)
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Account, searchQuery, stringResource(R.string.settings_category_account), "account", "login", "logout", "sign in", "saved", "provider", "membership", "subscription", "tv", "pair", "phone", "qr") {
                AccountSettingsPanel(
                    state = state,
                    viewModel = viewModel,
                    onOpenTvPairing = { onSelectCategory(SettingsCategory.TvPairing) },
                    searchMode = searchQuery.isNotBlank(),
                )
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.TvPairing, searchQuery, stringResource(R.string.tv_pair_settings_title), "tv", "pair", "phone", "qr", "code", "network") {
                LocalTvSettingsPanel(
                    state = state,
                    viewModel = viewModel,
                    showTitle = false,
                )
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Advanced, searchQuery, stringResource(R.string.settings_experimental_streaming), "experimental", "stream", "nvst", "l4s", "session", "launch", "failure") {
                Text(
                    stringResource(R.string.settings_experimental_streaming_warning),
                    color = SettingsTextMuted,
                    style = MaterialTheme.typography.labelSmall,
                )
                SettingSwitch(
                    label = "NVST transport (Experimental)",
                    checked = settings.stream.experimentalNvst,
                    description = "Try native NVST streaming for new sessions. Off by default while compatibility and stability issues are investigated.",
                ) {
                    viewModel.updateStreamSettings { s -> s.copy(experimentalNvst = it) }
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_l4s),
                    checked = settings.stream.enableL4S,
                    description = stringResource(R.string.settings_l4s_desc),
                ) {
                    viewModel.updateStreamSettings { s -> s.copy(enableL4S = it) }
                }
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.Advanced, searchQuery, "Codec Diagnostics", "codec", "diagnostics", "probe", "av1", "h264", "h265", "hevc", "decode") {
                    CodecDiagnosticsPanel(state.codecReport)
                }
    CategorySettingsSection(selectedCategory, SettingsCategory.Advanced, searchQuery, "Debug Logs", "debug", "logs", "logcat", "events", "export", "json", "cloudmatch", "queue", "stream") {
                    DebugLogsPanel(state = state, viewModel = viewModel)
                }
    if (deviceHasBattery) {
        CategorySettingsSection(selectedCategory, SettingsCategory.Advanced, searchQuery, "Battery Optimization", "battery", "optimization", "background", "activity", "ignore", "allow", "run") {
                    BatteryOptimizationPanel()
                }
    }
    CategorySettingsSection(selectedCategory, SettingsCategory.About, searchQuery, stringResource(R.string.settings_category_about), "about", "version", "build", "app", "github", "developer", "kiefer", "zortos", "opennow", "repository") {
                AppVersionPanel(settings = settings, onSettingsChange = viewModel::updateSettings)
                OpenNowGitHubPanel()
                DeveloperPanel()
            }
    CategorySettingsSection(selectedCategory, SettingsCategory.About, searchQuery, stringResource(R.string.settings_section_thanks), "thanks", "credits", "contributors", "darkevilpt", "discord", "community", "support", "donate", "paypal", "printedwaste") {
                ThanksPanel()
            }
    if (settings.developerOptionsUnlocked) {
        CategorySettingsSection(selectedCategory, SettingsCategory.Developer, searchQuery, stringResource(R.string.settings_category_developer), "developer", "developer options", "debug", "reset", "wipe", "diagnostics", "runtime", "environment", "flows", "onboarding", "cache") {
                    DeveloperOptionsPanel(state = state, viewModel = viewModel)
                }
    }
    }
}

@Composable
private fun DeviceStreamRecommendationWarning(
    reasons: List<String>,
    recommended: StreamSettings?,
) {
    val warningColor = Color(0xffffc266)
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(14.dp),
        color = warningColor.copy(alpha = 0.10f),
        contentColor = SettingsText,
        border = BorderStroke(1.dp, warningColor.copy(alpha = 0.38f)),
    ) {
        Column(
            modifier = Modifier.padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(
                stringResource(R.string.settings_above_recommendation),
                color = warningColor,
                fontWeight = FontWeight.Bold,
                style = MaterialTheme.typography.labelLarge,
            )
            Text(
                reasons.joinToString(", "),
                color = SettingsText,
                style = MaterialTheme.typography.bodyMedium,
            )
            Text(
                recommended?.let {
                    "Use Recommended (${it.recommendationSummary()}) and restart the stream before reporting lag. You can still use Custom and send a report after acknowledging the warning."
                } ?: "The Recommended preset is the safer option for lag troubleshooting.",
                color = SettingsTextMuted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
private fun SettingsCategoryLanding(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    categories: List<SettingsCategory>,
    onSelectCategory: (SettingsCategory) -> Unit,
) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        SettingsAccountCard(
            state = state,
            onClick = { onSelectCategory(SettingsCategory.Account) },
        )
        AndroidUpdateNoticeRow(
            update = state.androidUpdate,
            dismissedKey = state.dismissedAndroidUpdateNoticeKey,
            onOpenUpdates = { onSelectCategory(SettingsCategory.General) },
            onDismiss = viewModel::dismissAndroidUpdateNotice,
        )
        Surface(
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(28.dp),
            color = MaterialTheme.colorScheme.surface,
        ) {
            Column(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
                categories.filterNot { it == SettingsCategory.Account }.forEach { category ->
                    SettingsCategoryRow(
                        category = category,
                        onClick = { onSelectCategory(category) },
                    )
                }
            }
        }
        // About/Thanks used to be pasted in here because the category had no row of its own. It has
        // one now, so this duplicate is gone and About renders once, inside its own category.
    }
}

@Composable
private fun AdvancedOptionsSettings(settings: AppSettings, viewModel: OpenNowViewModel) {
    SettingSwitch(
        label = stringResource(R.string.settings_nerd_mode),
        checked = settings.nerdMode,
        description = stringResource(R.string.settings_nerd_mode_desc),
    ) {
        viewModel.updateSettings(settings.copy(nerdMode = it))
    }
}

@Composable
private fun CatalogBackgroundSettings(settings: AppSettings, viewModel: OpenNowViewModel) {
    val choices = listOf(
        AppBackgroundChoice.Default to stringResource(R.string.setup_background_default),
        AppBackgroundChoice.Nothing to stringResource(R.string.setup_background_nothing),
        AppBackgroundChoice.Wallpaper to stringResource(R.string.settings_background_wallpaper),
        AppBackgroundChoice.SystemWallpaper to stringResource(R.string.settings_background_system_wallpaper),
    )
    ChoiceRow(
        label = stringResource(R.string.settings_background_style),
        options = choices.map { it.second },
        selected = choices.first { it.first == appBackgroundChoiceFor(settings) }.second,
    ) { selectedLabel ->
        choices.firstOrNull { it.second == selectedLabel }?.first?.let { choice ->
            viewModel.updateSettings(settings.withAppBackgroundChoice(choice))
        }
    }
    // Keep the choices discoverable while the backdrop is off. Choosing an image or preset turns
    // the backdrop on, so users do not have to know that the switch used to gate these controls.
    CatalogBackgroundPicker(settings = settings, onSettingsChange = viewModel::updateSettings)
}

/** Est. bandwidth a stream at [mbps] pulls per hour, shared by Settings and the in-stream panel. */
internal fun streamBitrateUsageEstimate(mbps: Float): String =
    "Est. data usage: %.1f GB/hour".format((mbps * 3600f) / (8f * 1000f))

@Composable
private fun streamPresetLabel(preset: StreamPreset): String =
    when (preset) {
        StreamPreset.Recommended -> stringResource(R.string.stream_preset_recommended)
        StreamPreset.Custom -> stringResource(R.string.stream_preset_custom)
        StreamPreset.LowDataSaver -> stringResource(R.string.stream_preset_low_data_saver)
        StreamPreset.Medium -> stringResource(R.string.stream_preset_medium)
        StreamPreset.High -> stringResource(R.string.stream_preset_high)
    }

@Composable
private fun introMusicStartModeLabel(mode: IntroMusicStartMode): String =
    when (mode) {
        IntroMusicStartMode.Muted -> stringResource(R.string.intro_music_start_muted)
        IntroMusicStartMode.Playing -> stringResource(R.string.intro_music_start_playing)
    }

@Composable
private fun appLaunchPageLabel(page: AppLaunchPage): String =
    when (page) {
        AppLaunchPage.Store -> stringResource(R.string.launch_page_store)
        AppLaunchPage.Library -> stringResource(R.string.launch_page_library)
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
        ?: state.authSession?.user?.membershipTier?.takeIf { it.isNotBlank() }
        ?: account?.membershipTier?.takeIf { it.isNotBlank() }
    val detail = listOfNotNull(email, tier).joinToString(" - ").ifBlank {
        if (state.authSession == null && account == null) "Sign in to sync your GeForce NOW account" else "Manage account"
    }
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(28.dp)
    Box(Modifier.fillMaxWidth()) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .clip(shape)
                .onFocusChanged { focused = it.isFocused }
                .clickable(onClick = onClick),
            shape = shape,
            color = if (focused) MaterialTheme.colorScheme.surfaceVariant else MaterialTheme.colorScheme.surface,
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
        InteractionFocusFrame(
            visible = focused,
            cornerRadius = 28.dp,
            cinemaEffectEnabled = LocalAbsoluteCinemaEffects.current,
        )
    }
}

@Composable
private fun SettingsCategoryRow(category: SettingsCategory, onClick: () -> Unit) {
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(14.dp)
    val accent = MaterialTheme.colorScheme.primary
    Box(
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(shape)
                .onFocusChanged { focused = it.isFocused || it.hasFocus }
                .background(if (focused) accent.copy(alpha = 0.22f) else Color.Transparent)
                .clickable(onClick = onClick)
                .padding(horizontal = 20.dp, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(18.dp),
        ) {
            Surface(
                modifier = Modifier.size(42.dp),
                shape = RoundedCornerShape(14.dp),
                color = if (focused) accent else accent.copy(alpha = 0.16f),
            ) {
                Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxSize()) {
                    Icon(
                        imageVector = category.icon,
                        contentDescription = null,
                        tint = if (focused) MaterialTheme.colorScheme.onPrimary else accent,
                        modifier = Modifier.size(22.dp),
                    )
                }
            }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    stringResource(category.titleRes),
                    color = if (focused) Color.White else SettingsText,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = if (focused) FontWeight.ExtraBold else FontWeight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    stringResource(category.summaryRes),
                    color = if (focused) Color.White.copy(alpha = 0.86f) else SettingsTextMuted,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Icon(
                painter = painterResource(R.drawable.ic_chevron_right),
                contentDescription = null,
                tint = if (focused) accent else SettingsTextMuted,
                modifier = Modifier.size(22.dp),
            )
        }
        InteractionFocusFrame(
            visible = focused,
            cornerRadius = 14.dp,
            cinemaEffectEnabled = LocalAbsoluteCinemaEffects.current,
        )
    }
}

@Composable
private fun SettingsDetailHeader(
    category: SettingsCategory,
    tvProfile: Boolean,
    controllerFamily: AndroidControllerFamily?,
    onBack: () -> Unit,
) {
    val controllerNavigationEnabled = LocalSettingsControllerNavigationEnabled.current
    val showHardwareBackHint = tvProfile || controllerNavigationEnabled
    var backFocused by remember { mutableStateOf(false) }
    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (showHardwareBackHint) {
            Surface(
                modifier = Modifier
                    .onFocusChanged { backFocused = it.isFocused || it.hasFocus }
                    .border(
                        width = if (backFocused) 3.dp else 1.dp,
                        color = cinemaBorderColor(
                            LocalAbsoluteCinemaEffects.current,
                            LocalActiveSelectionColor.current,
                        ),
                        shape = RoundedCornerShape(999.dp),
                    )
                    .clickable(onClick = onBack),
                shape = RoundedCornerShape(999.dp),
                color = if (backFocused) MaterialTheme.colorScheme.primary.copy(alpha = 0.24f) else Color.Transparent,
            ) {
                Row(
                    // The badge touches the capsule edge so its white ring and the focused outer
                    // ring read as one continuous controller affordance instead of two outlines
                    // separated by a dark crescent.
                    modifier = Modifier.padding(end = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    val backBadgeColor = when (controllerFamily) {
                        null, AndroidControllerFamily.Google -> Color.White
                        AndroidControllerFamily.Xbox -> Color(0xFFFFC107)
                        AndroidControllerFamily.PlayStation -> Color(0xFFE94B5F)
                        AndroidControllerFamily.Nintendo -> Color(0xFFE60012)
                        AndroidControllerFamily.Generic -> MaterialTheme.colorScheme.primary
                    }
                    Surface(
                        modifier = Modifier
                            .size(38.dp)
                            .border(
                                2.dp,
                                cinemaBorderColor(
                                    LocalAbsoluteCinemaEffects.current,
                                    LocalActiveSelectionColor.current,
                                ),
                                CircleShape,
                            ),
                        shape = CircleShape,
                        color = backBadgeColor,
                    ) {
                        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                            when (controllerFamily) {
                                null, AndroidControllerFamily.Google -> Icon(
                                    painter = painterResource(R.drawable.ic_arrow_back),
                                    contentDescription = stringResource(R.string.cd_remote_back),
                                    tint = Color.Black,
                                    modifier = Modifier.size(17.dp),
                                )
                                AndroidControllerFamily.PlayStation -> Text(
                                    "○",
                                    color = Color.White,
                                    fontWeight = FontWeight.Black,
                                    style = MaterialTheme.typography.titleMedium,
                                )
                                AndroidControllerFamily.Xbox,
                                AndroidControllerFamily.Nintendo,
                                AndroidControllerFamily.Generic,
                                -> Text(
                                    "B",
                                    color = if (controllerFamily == AndroidControllerFamily.Xbox) Color.Black else Color.White,
                                    fontWeight = FontWeight.Black,
                                    style = MaterialTheme.typography.labelLarge,
                                )
                            }
                        }
                    }
                    Text(
                        stringResource(R.string.remote_back_label),
                        color = SettingsText,
                        fontWeight = FontWeight.Bold,
                        style = MaterialTheme.typography.labelSmall,
                        modifier = Modifier.padding(end = 7.dp),
                    )
                }
            }
        } else {
            Surface(
                modifier = Modifier
                    .size(42.dp)
                    .border(1.dp, SettingsTextMuted.copy(alpha = 0.5f), CircleShape)
                    .clickable(onClick = onBack),
                shape = CircleShape,
                color = Color.Transparent,
            ) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Icon(
                        painter = painterResource(R.drawable.ic_arrow_back),
                        contentDescription = stringResource(R.string.action_back),
                        tint = SettingsText,
                        modifier = Modifier.size(21.dp),
                    )
                }
            }
        }
        Column(Modifier.fillMaxWidth()) {
            Text(
                stringResource(category.titleRes),
                color = SettingsText,
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                stringResource(category.summaryRes),
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

/**
 * Every category the user can currently reach. Account was previously reachable only via the
 * account card, and About had no row at all — its content was instead duplicated inline into the
 * landing list, so it rendered twice in the body while being absent from the category list.
 *
 * Developer options are absent until the About build-number gesture unlocks them, and disappear
 * again when hidden from inside the page.
 */
private fun settingsCategories(developerOptionsUnlocked: Boolean): List<SettingsCategory> =
    SettingsCategory.entries.filter {
        it != SettingsCategory.TvPairing &&
            (it != SettingsCategory.Developer || developerOptionsUnlocked)
    }

private fun settingsDetailCategories(developerOptionsUnlocked: Boolean): List<SettingsCategory> =
    SettingsCategory.entries.filter { it != SettingsCategory.Developer || developerOptionsUnlocked }

private fun settingsCategoryParent(category: SettingsCategory?): SettingsCategory? =
    if (category == SettingsCategory.TvPairing) SettingsCategory.Account else null

private fun settingsRouteDepth(category: SettingsCategory?): Int = when (category) {
    null -> 0
    SettingsCategory.TvPairing -> 2
    else -> 1
}
