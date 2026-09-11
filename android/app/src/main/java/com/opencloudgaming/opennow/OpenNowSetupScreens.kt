package com.opencloudgaming.opennow

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.text.style.TextAlign
import com.opencloudgaming.opennow.ui.theme.LocalReduceMotion
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import coil3.compose.AsyncImage
import com.opencloudgaming.opennow.ui.controls.ControlRow
import com.opencloudgaming.opennow.ui.controls.ControlRowStyle
import com.opencloudgaming.opennow.ui.controls.ControlRowLabels
import com.opencloudgaming.opennow.ui.controls.ControlSection
import com.opencloudgaming.opennow.ui.controls.LocalControlRowStyle
import com.opencloudgaming.opennow.ui.controls.controlRowStyle
import com.opencloudgaming.opennow.ui.theme.OpenNowRadius
import com.opencloudgaming.opennow.ui.theme.OpenNowSpacing
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlinx.coroutines.delay

/**
 * First-run setup.
 *
 * Runs after sign-in, over the whole app, and writes every choice straight through to
 * [AppSettings] as it is made. Applying live is what makes the appearance step a real preview
 * rather than a mock-up of one: the screen behind the content *is* [CatalogWallpaperBackdrop] with
 * the user's current pick, the box art on it comes from their own catalog, and the accent
 * recolours this flow's chrome as they choose it. Waiting until after sign-in is what buys that
 * artwork — before it there is no catalog to preview against.
 *
 * Step order, gating, and the settings written on exit live in `AndroidSetupFlow.kt`.
 */
@Composable
internal fun SetupFlowScreen(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val settings = state.settings
    val tvProfile = state.androidTvProfile
    val controllerNavigationEnabled = tvProfile || rememberPhysicalControllerConnected(enabled = true)
    val focusManager = LocalFocusManager.current
    val primaryFocusRequester = remember { FocusRequester() }
    val reduceMotion = LocalReduceMotion.current
    var step by rememberSaveable { mutableStateOf(SetupStep.Welcome) }
    fun finish() {
        viewModel.updateSettings(settings.completingSetupFlow())
    }

    BackHandler(enabled = step != SetupStep.Welcome) {
        setupStepBefore(step)?.let { step = it }
    }
    // Land on the primary action rather than the header's Skip button. The requester attaches once
    // the step is laid out, so retry rather than betting on a single delay.
    LaunchedEffect(step) {
        repeat(6) { attempt ->
            if (runCatching { primaryFocusRequester.requestFocus() }.isSuccess) return@LaunchedEffect
            if (attempt < 5) delay(80)
        }
    }

    val edgePadding = if (tvProfile) {
        OpenNowSpacing.xl + settings.tvSafeAreaPaddingDp.coerceIn(0f, 120f).dp
    } else {
        OpenNowSpacing.lg
    }

    CompositionLocalProvider(
        LocalSettingsControllerNavigationEnabled provides controllerNavigationEnabled,
        // Settings rows are translucent because in Settings they sit on a flat background. Here
        // they sit on the user's wallpaper, where 76% opacity puts box art behind the labels.
        // Opaque rows keep the picture vivid everywhere it is not covering text.
        LocalControlRowStyle provides ControlRowStyle.settings().let { style ->
            style.copy(
                containerRest = MaterialTheme.colorScheme.surfaceVariant,
                containerFocused = MaterialTheme.colorScheme.surfaceVariant,
            )
        },
    ) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val wideLayout = maxWidth >= 720.dp
            val systemWallpaperVisible = shouldShowSystemWallpaperBackground(settings, inStream = false)
            if (!systemWallpaperVisible) {
                Box(Modifier.matchParentSize().background(MaterialTheme.colorScheme.background))
            }
            if (settings.nerdCatalogBackground) {
                CatalogWallpaperBackdrop(
                    settings = settings,
                    tvProfile = tvProfile,
                    width = maxWidth,
                    height = maxHeight,
                )
                // The wallpaper's own scrim is cut for artwork sitting on it, not for paragraphs.
                // Weight this one towards the bottom so the controls stay readable while the top of
                // the picture — the part being chosen — comes through close to untouched.
                val background = MaterialTheme.colorScheme.background
                Box(
                    Modifier
                        .matchParentSize()
                        .background(
                            Brush.verticalGradient(
                                listOf(
                                    background.copy(alpha = 0.25f),
                                    background.copy(alpha = 0.6f),
                                    background.copy(alpha = 0.88f),
                                ),
                            ),
                        ),
                )
            } else if (settings.ambientBackgroundEnabled) {
                AmbientBackground()
            }
            Column(
                Modifier
                    .fillMaxSize()
                    .systemBarsPadding()
                    .padding(horizontal = edgePadding, vertical = OpenNowSpacing.lg),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Column(
                    Modifier
                        .widthIn(max = SetupContentMaxWidth)
                        .fillMaxSize()
                        // On the whole flow, not just the scrolling part: a D-pad has to be able to
                        // cross between the step's controls and the footer, and Compose's default
                        // vertical search does not reliably step into and out of the animated
                        // content container on its own.
                        .onPreviewKeyEvent { handleVerticalDpadFocusMove(it, focusManager) },
                    verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
                ) {
                    SetupProgressBar(step, onSkip = { finish() })
                    Box(Modifier.weight(1f).fillMaxWidth()) {
                        AnimatedContent(
                            targetState = step,
                            modifier = Modifier.fillMaxSize(),
                            transitionSpec = {
                                val direction = if (targetState.ordinal > initialState.ordinal) 1 else -1
                                if (reduceMotion) {
                                    fadeIn(tween(0)) togetherWith fadeOut(tween(0))
                                } else {
                                    ((fadeIn(tween(220, delayMillis = 60)) +
                                        slideInHorizontally(tween(280)) { direction * it / 12 }) togetherWith
                                        (fadeOut(tween(100)) +
                                            slideOutHorizontally(tween(220)) { -direction * it / 12 }))
                                        .using(SizeTransform(clip = true))
                                }
                            },
                            label = "setup-step",
                        ) { currentStep ->
                            if (currentStep == SetupStep.Welcome) {
                                SetupWelcomeStep(wideLayout = wideLayout)
                            } else {
                                Column(
                                    Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
                                    verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
                                ) {
                                    SetupStepHeading(currentStep)
                                    when (currentStep) {
                                        SetupStep.Appearance -> SetupAppearanceStep(
                                            state = state,
                                            onSettingsChange = viewModel::updateSettings,
                                        )
                                        SetupStep.Streaming -> SetupStreamingStep(
                                            state = state,
                                            viewModel = viewModel,
                                        )
                                        SetupStep.Play -> SetupPlayStep(
                                            settings = settings,
                                            tvProfile = tvProfile,
                                            onSettingsChange = viewModel::updateSettings,
                                        )
                                        else -> {
                                            SetupReadyStep(settings = settings, tvProfile = tvProfile)
                                            SetupExpandableSection(stringResource(R.string.setup_feedback_reporter_title)) {
                                                SetupFeedbackStep(settings, viewModel::updateSettings)
                                            }
                                        }
                                    }
                                    Spacer(Modifier.height(OpenNowSpacing.sm))
                                }
                            }
                        }
                    }
                    SetupStepFooter(
                        step = step,
                        primaryFocusRequester = primaryFocusRequester,
                        onBack = { setupStepBefore(step)?.let { step = it } },
                        onNext = {
                            val next = setupStepAfter(step)
                            if (next == null) {
                                finish()
                            } else {
                                step = next
                            }
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun SetupProgressBar(step: SetupStep, onSkip: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            OpenNowMark(size = 24.dp)
            Spacer(Modifier.width(OpenNowSpacing.sm))
            Text(
                stringResource(R.string.app_name),
                modifier = Modifier.weight(1f),
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.Bold,
            )
            Text(
                "${step.ordinal + 1} / ${setupSteps().size}",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (!isFinalSetupStep(step)) {
                Spacer(Modifier.width(OpenNowSpacing.sm))
                TextButton(onClick = onSkip) {
                    Text(stringResource(R.string.setup_action_skip))
                }
            }
        }
        Row(Modifier.fillMaxWidth().semantics {
            progressBarRangeInfo = ProgressBarRangeInfo(
                current = (step.ordinal + 1).toFloat(),
                range = 0f..setupSteps().size.toFloat(),
                steps = setupSteps().size - 1,
            )
        }, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            setupSteps().forEach { candidate ->
                Box(
                    Modifier
                        .weight(1f)
                        .height(3.dp)
                        .clip(CircleShape)
                        .background(
                            if (candidate.ordinal <= step.ordinal) MaterialTheme.colorScheme.primary
                            else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.18f),
                        ),
                )
            }
        }
    }
}

@Composable
private fun SetupStepHeading(step: SetupStep) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(
            stringResource(step.titleRes),
            modifier = Modifier.semantics { heading() },
            color = MaterialTheme.colorScheme.onBackground,
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold,
        )
        Text(
            stringResource(step.subtitleRes),
            // Brighter than the usual muted body: this line sits directly on the user's wallpaper
            // rather than on a panel, and onSurfaceVariant loses against busy artwork.
            color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.82f),
            style = MaterialTheme.typography.bodyMedium,
        )
    }
}

@Composable
private fun SetupStepFooter(
    step: SetupStep,
    primaryFocusRequester: FocusRequester,
    onBack: () -> Unit,
    onNext: () -> Unit,
) {
    Surface(
        color = MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(OpenNowRadius.lg),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f)),
    ) {
        Row(
            Modifier.fillMaxWidth().padding(OpenNowSpacing.sm),
            horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (step != SetupStep.Welcome) {
                TextButton(onClick = onBack) {
                    Text(stringResource(R.string.setup_action_back))
                }
            }
            Spacer(Modifier.weight(1f))
            Button(
                onClick = onNext,
                modifier = Modifier.widthIn(min = 112.dp).focusRequester(primaryFocusRequester),
            ) {
                Text(
                    stringResource(
                        when {
                            step == SetupStep.Welcome -> R.string.setup_action_start
                            isFinalSetupStep(step) -> R.string.setup_action_finish
                            else -> R.string.setup_action_next
                        },
                    ),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun SetupWelcomeStep(wideLayout: Boolean) {
    // Scroll on short landscape displays and with large text instead of clipping the hero.
    BoxWithConstraints(Modifier.fillMaxSize()) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState())
                .heightIn(min = maxHeight).padding(vertical = OpenNowSpacing.lg),
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.lg, Alignment.CenterVertically),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Surface(
                shape = RoundedCornerShape(32.dp),
                color = MaterialTheme.colorScheme.primary.copy(alpha = 0.1f),
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.2f)),
            ) {
                Box(Modifier.padding(24.dp)) {
                    OpenNowMark(size = if (wideLayout) 80.dp else 64.dp)
                }
            }
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
            ) {
                Text(
                    stringResource(R.string.app_name),
                    modifier = Modifier.semantics { heading() },
                    color = MaterialTheme.colorScheme.onBackground,
                    style = MaterialTheme.typography.displaySmall,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    stringResource(R.string.setup_welcome_tagline),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.titleMedium,
                    textAlign = TextAlign.Center,
                )
            }
            Surface(
                modifier = Modifier.widthIn(max = 440.dp).fillMaxWidth(),
                shape = RoundedCornerShape(OpenNowRadius.lg),
                color = MaterialTheme.colorScheme.surface,
            ) {
                Column(
                    Modifier.padding(OpenNowSpacing.lg),
                    verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
                ) {
                    listOf(SetupStep.Appearance, SetupStep.Streaming, SetupStep.Play).forEachIndexed { index, next ->
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                "0${index + 1}",
                                color = MaterialTheme.colorScheme.primary,
                                style = MaterialTheme.typography.labelLarge,
                                modifier = Modifier.width(40.dp),
                            )
                            Text(stringResource(next.titleRes), style = MaterialTheme.typography.titleMedium)
                        }
                    }
                }
            }
            Text(
                stringResource(R.string.setup_ready_subtitle),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
                textAlign = TextAlign.Center,
            )
        }
    }
}

@Composable
private fun SetupExpandableSection(title: String, content: @Composable () -> Unit) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
        TextButton(
            onClick = { expanded = !expanded },
            modifier = Modifier.fillMaxWidth().semantics {
                if (expanded) collapse { expanded = false; true }
                else expand { expanded = true; true }
            },
        ) {
            Text(title, modifier = Modifier.weight(1f), textAlign = TextAlign.Start)
            Text(if (expanded) "−" else "+", style = MaterialTheme.typography.titleLarge)
        }
        if (expanded) content()
    }
}

@Composable
private fun SetupAppearanceStep(state: OpenNowUiState, onSettingsChange: (AppSettings) -> Unit) {
    val settings = state.settings
    val context = LocalContext.current
    val launchImagePicker = rememberCatalogBackgroundImagePicker(settings, onSettingsChange)
    val customUri = settings.nerdCatalogBackgroundUri?.takeIf { it.isNotBlank() }
    val backgroundName = when {
        appBackgroundChoiceFor(settings) == AppBackgroundChoice.Default ->
            stringResource(R.string.setup_background_default)
        appBackgroundChoiceFor(settings) == AppBackgroundChoice.Nothing ->
            stringResource(R.string.setup_background_nothing)
        appBackgroundChoiceFor(settings) == AppBackgroundChoice.SystemWallpaper ->
            stringResource(R.string.settings_background_system_wallpaper)
        customUri != null -> stringResource(R.string.settings_catalog_background_image_custom)
        else -> catalogBackgroundPresetLabel(settings.catalogBackgroundPreset)
    }
    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.md),
    ) {
        SetupAppearancePreview(state)

        Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
            SetupSectionLabel(stringResource(R.string.setup_background), backgroundName)
            SetupPeekRow { tileWidth ->
                SetupTile(
                    width = tileWidth,
                    selected = appBackgroundChoiceFor(settings) == AppBackgroundChoice.Default,
                    onClick = { onSettingsChange(settings.withAppBackgroundChoice(AppBackgroundChoice.Default)) },
                ) {
                    Box(Modifier.matchParentSize().background(MaterialTheme.colorScheme.background))
                    AmbientBackground(Modifier.matchParentSize())
                    Text(
                        stringResource(R.string.setup_background_default),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.labelMedium,
                    )
                }
                SetupTile(
                    width = tileWidth,
                    selected = appBackgroundChoiceFor(settings) == AppBackgroundChoice.Nothing,
                    onClick = { onSettingsChange(settings.withAppBackgroundChoice(AppBackgroundChoice.Nothing)) },
                ) {
                    Box(Modifier.matchParentSize().background(MaterialTheme.colorScheme.background))
                    Text(
                        stringResource(R.string.setup_background_nothing),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.labelMedium,
                    )
                }
                SetupTile(
                    width = tileWidth,
                    selected = appBackgroundChoiceFor(settings) == AppBackgroundChoice.SystemWallpaper,
                    onClick = {
                        onSettingsChange(settings.withAppBackgroundChoice(AppBackgroundChoice.SystemWallpaper))
                    },
                ) {
                    Text(
                        stringResource(R.string.settings_background_system_wallpaper),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.labelMedium,
                    )
                }
                CatalogBackgroundPreset.entries.forEach { preset ->
                    SetupTile(
                        width = tileWidth,
                        selected = settings.nerdCatalogBackground &&
                            customUri == null &&
                            settings.catalogBackgroundPreset == preset,
                        onClick = { applyCatalogBackgroundPreset(context, settings, preset, onSettingsChange) },
                    ) {
                        Image(
                            painter = painterResource(preset.drawableRes),
                            contentDescription = catalogBackgroundPresetLabel(preset),
                            modifier = Modifier.matchParentSize(),
                            contentScale = ContentScale.Crop,
                        )
                    }
                }
                SetupTile(
                    width = tileWidth,
                    selected = settings.nerdCatalogBackground && customUri != null,
                    onClick = launchImagePicker,
                ) {
                    if (customUri == null) {
                        Box(
                            Modifier
                                .matchParentSize()
                                .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.7f)),
                        )
                        Text(
                            stringResource(R.string.setup_background_your_image),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            style = MaterialTheme.typography.labelMedium,
                        )
                    } else {
                        AsyncImage(
                            model = imageDataForSource(customUri),
                            contentDescription = stringResource(R.string.settings_catalog_background_image_custom),
                            modifier = Modifier.matchParentSize(),
                            contentScale = ContentScale.Crop,
                        )
                    }
                }
            }
        }

        Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
            SetupSectionLabel(
                stringResource(R.string.setup_appearance_accent),
                uiAccentLabel(settings.uiAccent),
            )
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
            ) {
                selectableUiAccents().forEach { accent ->
                    SetupAccentSwatch(
                        accent = accent,
                        selected = settings.uiAccent == accent,
                        onClick = { onSettingsChange(settings.copy(uiAccent = accent)) },
                    )
                }
            }
        }

        SetupExpandableSection(stringResource(R.string.setup_appearance_layout)) {
            Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
                SetupSectionLabel(
                    stringResource(R.string.setup_appearance_layout),
                    stringResource(R.string.setup_appearance_layout_hint),
                )
                SettingSwitch(
                    label = stringResource(R.string.setup_appearance_titles),
                    checked = settings.showCardTitles,
                    description = stringResource(R.string.setup_appearance_titles_desc),
                ) {
                    onSettingsChange(settings.copy(showCardTitles = it))
                }
                SettingSwitch(
                    label = stringResource(R.string.setup_appearance_compact),
                    checked = settings.compactGameCards,
                    description = stringResource(R.string.setup_appearance_compact_desc),
                ) {
                    onSettingsChange(settings.copy(compactGameCards = it))
                }
                SettingSwitch(
                    label = stringResource(R.string.setup_appearance_favorite_icon),
                    checked = settings.showFavoriteIconOnGameCards,
                    description = stringResource(R.string.setup_appearance_favorite_icon_desc),
                ) {
                    onSettingsChange(settings.copy(showFavoriteIconOnGameCards = it))
                }
                SettingSwitch(
                    label = stringResource(R.string.setup_appearance_expressive),
                    checked = settings.expressiveUi,
                    description = stringResource(R.string.setup_appearance_expressive_desc),
                ) {
                    onSettingsChange(settings.copy(expressiveUi = it))
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_live_selected_outlines),
                    checked = settings.liveSelectedOutlines,
                    description = stringResource(R.string.settings_live_selected_outlines_desc),
                ) {
                    onSettingsChange(settings.copy(liveSelectedOutlines = it))
                }
                SettingSwitch(
                    label = stringResource(R.string.settings_absolute_cinema_effects),
                    checked = settings.absoluteCinemaEffects,
                    description = stringResource(R.string.settings_absolute_cinema_effects_desc),
                ) { enabled ->
                    onSettingsChange(
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
                ) {
                    onSettingsChange(settings.copy(absoluteCinemaEverywhere = it))
                }
                SettingSwitch(
                    label = stringResource(R.string.setup_appearance_animations),
                    checked = settings.controllerBackgroundAnimations,
                    description = stringResource(R.string.setup_appearance_animations_desc),
                ) {
                    onSettingsChange(settings.copy(controllerBackgroundAnimations = it))
                }
            }
        }

        // Feedback belongs on this step rather than buried in Settings > Interface: both fire on
        // the very next tap, so setup is the one moment where turning them off costs nothing to
        // find out about.
        Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
            SetupSectionLabel(
                stringResource(R.string.setup_appearance_feedback),
                stringResource(R.string.setup_appearance_feedback_hint),
            )
            SettingSwitch(
                label = stringResource(R.string.setup_appearance_haptics),
                checked = settings.vibrationEnabled,
                description = stringResource(R.string.setup_appearance_haptics_desc),
            ) {
                onSettingsChange(settings.copy(vibrationEnabled = it))
            }
            SettingSwitch(
                label = stringResource(R.string.setup_appearance_sounds),
                checked = settings.controllerUiSounds,
                description = stringResource(R.string.setup_appearance_sounds_desc),
            ) {
                onSettingsChange(settings.copy(controllerUiSounds = it))
            }
        }
    }
}

/**
 * A working miniature of the app, on the user's own backdrop and box art.
 *
 * This replaces a flat strip of posters that showed the backdrop and nothing else. It is drawn as
 * the app rather than as a sample of one — its own window with the Store's top bar, a section
 * heading, the grid, and the tab bar underneath — because the choices on this step apply to the
 * chrome as much as to the cards, and a bare row of posters could not show that. Every switch below
 * redraws it: corner radius, captions, card shape, the favourite badge, and the frame on the
 * selected card.
 *
 * Non-interactive by design: it is the sample, and the controls below it are the step.
 *
 * Renders nothing until the catalogue has artwork. Placeholder blocks would preview nothing and
 * read as an unfinished screen.
 */
@Composable
private fun SetupAppearancePreview(state: OpenNowUiState) {
    val settings = state.settings
    val tvProfile = state.androidTvProfile
    val games = remember(state.games, state.libraryGames, tvProfile) {
        (state.libraryGames + state.games)
            .distinctBy { it.id }
            .filter { !catalogCardImageUrl(it, tvProfile).isNullOrBlank() }
            .take(SETUP_PREVIEW_CARD_COUNT)
    }
    if (games.isEmpty()) return
    val cardShape = RoundedCornerShape(if (settings.expressiveUi) OpenNowRadius.md else OpenNowRadius.sm)
    val windowShape = RoundedCornerShape(if (settings.expressiveUi) OpenNowRadius.lg else OpenNowRadius.sm)
    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.xs),
    ) {
        Text(
            stringResource(R.string.setup_appearance_preview_label),
            color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.72f),
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.SemiBold,
        )
        Surface(
            modifier = Modifier.fillMaxWidth(),
            shape = windowShape,
            // The system-wallpaper choice is a real preview: this surface becomes transparent and
            // Android keeps drawing the user's static or live wallpaper underneath it.
            color = if (settings.systemWallpaperBackground) {
                Color.Transparent
            } else {
                MaterialTheme.colorScheme.background
            },
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.onBackground.copy(alpha = 0.14f)),
        ) {
            Column(Modifier.fillMaxWidth()) {
                SetupPreviewTopBar()
                Column(
                    Modifier
                        .fillMaxWidth()
                        .padding(horizontal = OpenNowSpacing.sm),
                    verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.xs),
                ) {
                    Text(
                        stringResource(R.string.store_results),
                        color = MaterialTheme.colorScheme.onBackground,
                        style = MaterialTheme.typography.labelMedium,
                        fontWeight = FontWeight.Bold,
                    )
                    Row(
                        Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
                    ) {
                        games.forEachIndexed { index, game ->
                            // The first card stands in for the focused one, so the accent and the
                            // Absolute Cinema frame have somewhere to show.
                            val selected = index == 0
                            Column(
                                Modifier.weight(1f),
                                verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.xs),
                            ) {
                                Box(
                                    Modifier
                                        .fillMaxWidth()
                                        .aspectRatio(
                                            if (settings.compactGameCards) 1f else GAME_BOX_ART_ASPECT_RATIO,
                                        ),
                                ) {
                                    Box(
                                        Modifier
                                            .matchParentSize()
                                            .clip(cardShape)
                                            .then(
                                                if (
                                                    selected &&
                                                    settings.liveSelectedOutlines &&
                                                    !LocalAbsoluteCinemaEffects.current
                                                ) {
                                                    Modifier.border(
                                                        1.dp,
                                                        LocalSelectionTintColor.current,
                                                        cardShape,
                                                    )
                                                } else {
                                                    Modifier
                                                },
                                            ),
                                    ) {
                                        UrlImage(
                                            catalogCardImageUrl(game, tvProfile),
                                            Modifier.matchParentSize(),
                                            contentScale = ContentScale.Crop,
                                        )
                                        if (settings.showFavoriteIconOnGameCards) {
                                            SetupPreviewFavoriteBadge(
                                                Modifier
                                                    .align(Alignment.TopStart)
                                                    .padding(4.dp),
                                            )
                                        }
                                    }
                                    ControllerFocusFrame(
                                        visible = selected && LocalAbsoluteCinemaEffects.current,
                                        cornerRadius = if (settings.expressiveUi) OpenNowRadius.md else OpenNowRadius.sm,
                                        tint = LocalActiveSelectionColor.current,
                                        secondaryTint = LocalActiveSelectionSecondaryColor.current,
                                    )
                                }
                                if (settings.showCardTitles) {
                                    Text(
                                        game.title,
                                        color = MaterialTheme.colorScheme.onBackground,
                                        style = MaterialTheme.typography.labelSmall,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                }
                            }
                        }
                    }
                }
                SetupPreviewTabBar()
            }
        }
    }
}

/** The Store's top bar, reduced to the shapes that read at this size. */
@Composable
private fun SetupPreviewTopBar() {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = OpenNowSpacing.sm, vertical = OpenNowSpacing.sm),
        horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        OpenNowMark(size = 14.dp)
        Text(
            stringResource(R.string.nav_store),
            Modifier.weight(1f),
            color = MaterialTheme.colorScheme.onBackground,
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.Bold,
        )
        SetupPreviewChromeDot()
        SetupPreviewChromeDot()
    }
}

@Composable
private fun SetupPreviewChromeDot() {
    Box(
        Modifier
            .size(12.dp)
            .clip(CircleShape)
            .background(MaterialTheme.colorScheme.onBackground.copy(alpha = 0.22f)),
    )
}

/**
 * The tab bar, with Store selected.
 *
 * Worth drawing even though the accent no longer reaches it: showing the fixed navigation tint
 * beside an accent-coloured card is exactly how the two read in the real app, and a preview that
 * left the chrome out would imply the accent covers everything.
 */
@Composable
private fun SetupPreviewTabBar() {
    val labels = listOf(
        stringResource(R.string.nav_store) to true,
        stringResource(R.string.nav_library) to false,
        stringResource(R.string.nav_settings) to false,
    )
    Row(
        Modifier
            .fillMaxWidth()
            .padding(top = OpenNowSpacing.sm)
            .background(MaterialTheme.colorScheme.surface)
            .padding(vertical = 6.dp),
        horizontalArrangement = Arrangement.SpaceEvenly,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        labels.forEach { (label, selected) ->
            val tint = if (selected) {
                NavigationSelectionColor
            } else {
                MaterialTheme.colorScheme.onSurfaceVariant
            }
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Box(
                    Modifier
                        .size(width = 20.dp, height = 4.dp)
                        .clip(CircleShape)
                        .background(tint.copy(alpha = if (selected) 1f else 0.45f)),
                )
                Spacer(Modifier.height(3.dp))
                Text(
                    label,
                    color = tint,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun SetupPreviewFavoriteBadge(modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(20.dp)
            .clip(CircleShape)
            .background(Color.Black.copy(alpha = 0.42f)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            painter = painterResource(R.drawable.ic_save),
            contentDescription = null,
            tint = Color.White,
            modifier = Modifier.size(11.dp),
        )
    }
}

/**
 * A horizontally scrolling row of picture tiles that always cuts one off at the trailing edge.
 *
 * The tiles previously used a fixed 116dp width, which on a typical phone left the row *just*
 * filled: nothing was clipped, no scrollbar showed, and the options past the fold were invisible
 * unless the user happened to try dragging. Sizing the tiles from the available width instead
 * guarantees a partial one at the edge, which is the affordance — a cut-off tile is read as "there
 * is more" without any extra chrome. The fade reinforces it and disappears at the end of the row.
 */
@Composable
private fun SetupPeekRow(content: @Composable RowScope.(tileWidth: Dp) -> Unit) {
    val scrollState = rememberScrollState()
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val tileWidth = ((maxWidth - OpenNowSpacing.sm * SETUP_TILE_PEEK_COUNT) / SETUP_TILE_PEEK_COUNT)
            .coerceIn(SetupTileMinWidth, SetupTileMaxWidth)
        Row(
            Modifier.fillMaxWidth().horizontalScroll(scrollState),
            horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
        ) {
            content(tileWidth)
        }
        // Edge shadows on whichever side has more to reach. Drawn rather than composed: reading the
        // scroll position inside drawBehind keeps a drag in the draw phase instead of recomposing
        // the whole row — and every tile in it — on every frame of the gesture. Black rather than a
        // surface colour because this row sits directly on the user's wallpaper, where a flat
        // theme-coloured band would read as a smear and a shadow reads as depth.
        Box(
            Modifier
                .matchParentSize()
                .drawBehind {
                    val fade = SetupPeekFadeWidth.toPx().coerceAtMost(size.width)
                    if (scrollState.canScrollForward) {
                        drawRect(
                            Brush.horizontalGradient(
                                colors = listOf(Color.Transparent, SetupPeekFadeColor),
                                startX = size.width - fade,
                                endX = size.width,
                            ),
                        )
                    }
                    if (scrollState.canScrollBackward) {
                        drawRect(
                            Brush.horizontalGradient(
                                colors = listOf(SetupPeekFadeColor, Color.Transparent),
                                startX = 0f,
                                endX = fade,
                            ),
                        )
                    }
                },
        )
    }
}

@Composable
private fun SetupStreamingStep(state: OpenNowUiState, viewModel: OpenNowViewModel) {
    val settings = state.settings
    val recommended = state.recommendedStreamSettings
    val selected = setupStreamingChoiceFor(settings)
    val fallbackMembershipTier = state.authSession?.user?.membershipTier
    val entitlements = remember(state.subscriptionInfo, fallbackMembershipTier) {
        streamPlanEntitlements(state.subscriptionInfo, fallbackMembershipTier)
    }
    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
    ) {
        SetupMembershipCard(entitlements)
        SetupChoiceRow(
            label = stringResource(R.string.setup_streaming_recommended),
            value = recommended?.recommendationSummary()
                ?: stringResource(R.string.setup_streaming_measuring),
            selected = selected == SetupStreamingChoice.Recommended,
        ) {
            viewModel.applyStreamPreset(setupStreamingPresetFor(SetupStreamingChoice.Recommended))
        }
        SetupChoiceRow(
            label = stringResource(R.string.setup_streaming_best),
            value = stringResource(
                R.string.setup_streaming_best_desc,
                entitlements.maxResolutionLabel,
                entitlements.maxFps,
            ),
            selected = selected == SetupStreamingChoice.Best,
        ) {
            viewModel.applyStreamPreset(setupStreamingPresetFor(SetupStreamingChoice.Best))
        }
        SetupChoiceRow(
            label = stringResource(R.string.setup_streaming_data_saver),
            value = stringResource(R.string.setup_streaming_data_saver_desc),
            selected = selected == SetupStreamingChoice.DataSaver,
        ) {
            viewModel.applyStreamPreset(setupStreamingPresetFor(SetupStreamingChoice.DataSaver))
        }
        SetupChoiceRow(
            label = stringResource(R.string.setup_streaming_custom),
            value = if (selected == SetupStreamingChoice.Custom) {
                settings.stream.recommendationSummary()
            } else {
                stringResource(R.string.setup_streaming_custom_desc)
            },
            selected = selected == SetupStreamingChoice.Custom,
        ) {
            viewModel.applyStreamPreset(setupStreamingPresetFor(SetupStreamingChoice.Custom))
        }
        // Only under Custom. Beside a preset these would edit values the next preset write
        // discards, which reads as the controls not working.
        AnimatedVisibility(visible = setupStreamingCustomControlsVisible(selected)) {
            SetupCustomStreamControls(
                state = state,
                viewModel = viewModel,
                entitlements = entitlements,
                fallbackMembershipTier = fallbackMembershipTier,
            )
        }
    }
}

@Composable
private fun SetupPlayStep(
    settings: AppSettings,
    tvProfile: Boolean,
    onSettingsChange: (AppSettings) -> Unit,
) {
    val touchChoice = setupTouchMouseChoiceFor(settings)
    var fullScreenPreview by rememberSaveable { mutableStateOf(false) }
    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    stringResource(R.string.setup_play_preview),
                    color = MaterialTheme.colorScheme.onBackground,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    stringResource(R.string.setup_play_preview_hint),
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.72f),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Button(onClick = { fullScreenPreview = true }) {
                Text(stringResource(R.string.setup_play_fullscreen), maxLines = 1)
            }
        }
        SetupStreamExperiencePreview(
            settings = settings,
            showTouchHint = !tvProfile,
            modifier = Modifier.fillMaxWidth().aspectRatio(16f / 9f),
        )

        if (fullScreenPreview) {
            SetupFullScreenStreamPreview(
                settings = settings,
                showTouchHint = !tvProfile,
                onDismiss = { fullScreenPreview = false },
            )
        }

        if (!tvProfile) {
            SetupSectionLabel(
                title = stringResource(R.string.setup_play_touch),
                value = setupTouchMouseChoiceLabel(touchChoice),
            )
            SetupChoiceRow(
                label = stringResource(R.string.setup_play_touch_direct),
                value = stringResource(R.string.setup_play_touch_direct_desc),
                selected = touchChoice == SetupTouchMouseChoice.Direct,
            ) {
                onSettingsChange(settings.withSetupTouchMouseChoice(SetupTouchMouseChoice.Direct))
            }
            SetupChoiceRow(
                label = stringResource(R.string.setup_play_touch_trackpad),
                value = stringResource(R.string.setup_play_touch_trackpad_desc),
                selected = touchChoice == SetupTouchMouseChoice.Trackpad,
            ) {
                onSettingsChange(settings.withSetupTouchMouseChoice(SetupTouchMouseChoice.Trackpad))
            }
            SetupChoiceRow(
                label = stringResource(R.string.setup_play_touch_off),
                value = stringResource(R.string.setup_play_touch_off_desc),
                selected = touchChoice == SetupTouchMouseChoice.Off,
            ) {
                onSettingsChange(settings.withSetupTouchMouseChoice(SetupTouchMouseChoice.Off))
            }
        }

        SettingSwitch(
            label = stringResource(R.string.setup_play_status),
            checked = settings.showStatsOnLaunch,
            description = stringResource(R.string.setup_play_status_desc),
        ) { enabled ->
            onSettingsChange(settings.copy(showStatsOnLaunch = enabled))
        }
        if (settings.showStatsOnLaunch) {
            SetupExpandableSection(stringResource(R.string.stream_statusbar_items)) {
                Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
                    ChoiceMenuRow(
                        label = stringResource(R.string.stream_statusbar_appearance),
                        options = StreamStatsStyle.entries.map { style ->
                            ChoiceMenuOption(value = style.name, label = style.label)
                        },
                        selectedLabel = settings.streamStatsStyle.label,
                    ) { value ->
                        StreamStatsStyle.entries.firstOrNull { it.name == value }?.let { style ->
                            onSettingsChange(settings.copy(streamStatsStyle = style))
                        }
                    }
                    ChoiceMenuRow(
                        label = stringResource(R.string.setup_play_status_position),
                        options = StreamStatsPosition.entries.map { position ->
                            ChoiceMenuOption(value = position.name, label = position.label)
                        },
                        selectedLabel = settings.streamStatsPosition.label,
                    ) { value ->
                        StreamStatsPosition.entries.firstOrNull { it.name == value }?.let { position ->
                            onSettingsChange(settings.copy(streamStatsPosition = position))
                        }
                    }
                    SetupSectionLabel(
                        title = stringResource(R.string.stream_statusbar_items),
                        value = stringResource(
                            R.string.setup_play_status_items_selected,
                            StreamStatusItem.entries.count { it.enabledIn(settings) },
                        ),
                    )
                    SetupStreamStatusItems(
                        settings = settings,
                        onSettingsChange = onSettingsChange,
                    )
                }
            }
        }
    }
}

@Composable
private fun SetupFullScreenStreamPreview(
    settings: AppSettings,
    showTouchHint: Boolean,
    onDismiss: () -> Unit,
) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(
            usePlatformDefaultWidth = false,
            decorFitsSystemWindows = false,
        ),
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .background(Color.Black)
                .systemBarsPadding()
                .padding(8.dp),
        ) {
            SetupStreamExperiencePreview(
                settings = settings,
                showTouchHint = showTouchHint,
                expanded = true,
                modifier = Modifier.fillMaxSize(),
                onDismiss = onDismiss,
            )
        }
    }
}

/** A fake stream that lets setup choices be rehearsed without sending any input to a session. */
@Composable
private fun SetupStreamExperiencePreview(
    settings: AppSettings,
    showTouchHint: Boolean,
    modifier: Modifier,
    expanded: Boolean = false,
    onDismiss: (() -> Unit)? = null,
) {
    val touchChoice = setupTouchMouseChoiceFor(settings)
    val statusAlignment = when (settings.streamStatsPosition) {
        StreamStatsPosition.Left -> Alignment.TopStart
        StreamStatsPosition.Center -> Alignment.TopCenter
        StreamStatsPosition.Right -> Alignment.TopEnd
    }
    var cursorX by remember(expanded) { mutableStateOf(0.34f) }
    var cursorY by remember(expanded) { mutableStateOf(0.56f) }
    var targetHits by remember(expanded) { mutableStateOf(0) }
    var practiceMessage by remember(expanded) { mutableStateOf<String?>(null) }
    var fakeMenuOpen by remember(expanded) { mutableStateOf(false) }
    var fakeKeyboardOpen by remember(expanded) { mutableStateOf(false) }
    val moveToTargetMessage = stringResource(R.string.setup_play_move_to_target)

    fun cursorOnTarget(): Boolean = abs(cursorX - 0.5f) <= 0.13f && abs(cursorY - 0.48f) <= 0.16f
    fun clickCursor() {
        if (cursorOnTarget()) {
            targetHits += 1
            practiceMessage = null
        } else {
            practiceMessage = moveToTargetMessage
        }
    }

    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(if (expanded) OpenNowRadius.md else OpenNowRadius.lg),
        color = Color(0xFF08141C),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.25f)),
    ) {
        BoxWithConstraints(
            Modifier
                .fillMaxSize()
                .background(
                    Brush.verticalGradient(
                        listOf(Color(0xFF16384A), Color(0xFF0B2731), Color(0xFF071116)),
                    ),
                ),
        ) {
            SetupPracticeBackdrop(expanded = expanded)
            // This sibling is behind every fake button. It owns only the empty play field, so a
            // drag detector can never consume Menu/Keys/A/B or the fullscreen exit action.
            Box(
                Modifier
                    .matchParentSize()
                    .pointerInput(touchChoice) {
                    detectTapGestures { position ->
                        if (touchChoice == SetupTouchMouseChoice.Direct) {
                            cursorX = (position.x / size.width.toFloat()).coerceIn(0.02f, 0.98f)
                            cursorY = (position.y / size.height.toFloat()).coerceIn(0.04f, 0.96f)
                        }
                        clickCursor()
                    }
                    }
                    .pointerInput(touchChoice) {
                        detectDragGestures(
                            onDragStart = { position ->
                                if (touchChoice == SetupTouchMouseChoice.Direct) {
                                    cursorX = (position.x / size.width.toFloat()).coerceIn(0.02f, 0.98f)
                                    cursorY = (position.y / size.height.toFloat()).coerceIn(0.04f, 0.96f)
                                }
                            },
                            onDrag = { change, dragAmount ->
                                change.consume()
                                if (touchChoice == SetupTouchMouseChoice.Direct) {
                                    cursorX = (change.position.x / size.width.toFloat()).coerceIn(0.02f, 0.98f)
                                    cursorY = (change.position.y / size.height.toFloat()).coerceIn(0.04f, 0.96f)
                                } else {
                                    cursorX = (cursorX + dragAmount.x / size.width.toFloat()).coerceIn(0.02f, 0.98f)
                                    cursorY = (cursorY + dragAmount.y / size.height.toFloat()).coerceIn(0.04f, 0.96f)
                                }
                                practiceMessage = null
                            },
                        )
                    },
            )

            Surface(
                modifier = Modifier.align(Alignment.Center).offset(y = (-4).dp),
                shape = RoundedCornerShape(OpenNowRadius.full),
                color = if (cursorOnTarget()) {
                    MaterialTheme.colorScheme.primary
                } else {
                    MaterialTheme.colorScheme.primary.copy(alpha = 0.82f)
                },
                border = BorderStroke(1.dp, Color.White.copy(alpha = 0.3f)),
            ) {
                Text(
                    text = if (targetHits == 0) {
                        stringResource(R.string.setup_play_preview_target)
                    } else {
                        stringResource(R.string.setup_play_preview_hits, targetHits)
                    },
                    modifier = Modifier.padding(
                        horizontal = if (expanded) OpenNowSpacing.xl else OpenNowSpacing.lg,
                        vertical = if (expanded) 12.dp else 8.dp,
                    ),
                    color = MaterialTheme.colorScheme.onPrimary,
                    style = if (expanded) MaterialTheme.typography.titleSmall else MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                )
            }

            AnimatedVisibility(
                visible = settings.showStatsOnLaunch,
                modifier = Modifier.align(statusAlignment),
                enter = fadeIn(tween(120)),
                exit = fadeOut(tween(90)),
            ) {
                SetupFakeStatusLine(settings = settings, expanded = expanded)
            }

            if (showTouchHint && !expanded) {
                Surface(
                    modifier = Modifier.align(Alignment.BottomStart).padding(10.dp),
                    shape = RoundedCornerShape(OpenNowRadius.full),
                    color = Color.Black.copy(alpha = 0.48f),
                ) {
                    Text(
                        text = stringResource(
                            when (touchChoice) {
                                SetupTouchMouseChoice.Direct -> R.string.setup_play_preview_direct
                                SetupTouchMouseChoice.Trackpad -> R.string.setup_play_preview_trackpad
                                SetupTouchMouseChoice.Off -> R.string.setup_play_preview_off
                            },
                        ),
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                        color = Color.White.copy(alpha = 0.9f),
                        style = MaterialTheme.typography.labelSmall,
                    )
                }
            }

            SetupPracticeCursor(
                modifier = Modifier.offset(
                    x = (maxWidth - 18.dp) * cursorX,
                    y = (maxHeight - 24.dp) * cursorY,
                ),
            )

            if (fakeMenuOpen) {
                SetupPracticeMenu(
                    expanded = expanded,
                    onClose = { fakeMenuOpen = false },
                    modifier = Modifier.align(Alignment.CenterStart).padding(12.dp),
                )
            }

            if (fakeKeyboardOpen) {
                SetupPracticeKeyboard(
                    expanded = expanded,
                    modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = if (expanded) 66.dp else 46.dp),
                )
            }

            Column(
                modifier = Modifier.align(Alignment.BottomEnd).padding(if (expanded) 16.dp else 8.dp),
                horizontalAlignment = Alignment.End,
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                practiceMessage?.let { message ->
                    Text(
                        message,
                        color = Color.White.copy(alpha = 0.86f),
                        style = MaterialTheme.typography.labelSmall,
                    )
                }
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    SetupPracticeButton(
                        label = stringResource(R.string.setup_play_fake_menu),
                        compact = !expanded,
                    ) {
                        fakeMenuOpen = !fakeMenuOpen
                        fakeKeyboardOpen = false
                    }
                    SetupPracticeButton(
                        label = stringResource(R.string.setup_play_fake_keyboard),
                        compact = !expanded,
                    ) {
                        fakeKeyboardOpen = !fakeKeyboardOpen
                        fakeMenuOpen = false
                    }
                    SetupPracticeButton(label = "A", compact = !expanded, emphasized = true) {
                        clickCursor()
                    }
                    SetupPracticeButton(label = "B", compact = !expanded) {
                        cursorX = 0.34f
                        cursorY = 0.56f
                        targetHits = 0
                        practiceMessage = null
                    }
                }
            }

            if (expanded) {
                Surface(
                    modifier = Modifier.align(Alignment.TopCenter).padding(top = 54.dp),
                    shape = RoundedCornerShape(OpenNowRadius.full),
                    color = Color.Black.copy(alpha = 0.45f),
                ) {
                    Text(
                        stringResource(R.string.setup_play_practice_tip),
                        modifier = Modifier.padding(horizontal = 14.dp, vertical = 7.dp),
                        color = Color.White.copy(alpha = 0.86f),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                Button(
                    onClick = { onDismiss?.invoke() },
                    // Keep every status-line position unobstructed while the player evaluates it.
                    modifier = Modifier.align(Alignment.BottomStart).padding(12.dp),
                ) {
                    Text(stringResource(R.string.setup_play_leave_fullscreen))
                }
            }
        }
    }
}

@Composable
private fun SetupPracticeBackdrop(expanded: Boolean) {
    Canvas(Modifier.fillMaxSize()) {
        val horizon = size.height * 0.58f
        drawCircle(
            color = Color(0xFF3E91A2).copy(alpha = 0.16f),
            radius = size.minDimension * 0.34f,
            center = center.copy(y = horizon * 0.82f),
        )
        drawRect(
            color = Color.Black.copy(alpha = 0.18f),
            topLeft = center.copy(x = 0f, y = horizon),
            size = size.copy(height = size.height - horizon),
        )
        val lanes = if (expanded) 8 else 5
        repeat(lanes) { index ->
            val fraction = index / (lanes - 1f)
            drawLine(
                color = Color.White.copy(alpha = 0.055f),
                start = center.copy(x = size.width * fraction, y = horizon),
                end = center.copy(x = size.width * (fraction * 1.25f - 0.12f), y = size.height),
                strokeWidth = 1f,
            )
        }
    }
}

@Composable
private fun SetupPracticeCursor(modifier: Modifier = Modifier) {
    Canvas(modifier.size(18.dp, 24.dp)) {
        val cursor = Path().apply {
            moveTo(1f, 1f)
            lineTo(size.width * 0.78f, size.height * 0.62f)
            lineTo(size.width * 0.48f, size.height * 0.66f)
            lineTo(size.width * 0.68f, size.height - 1f)
            lineTo(size.width * 0.48f, size.height - 1f)
            lineTo(size.width * 0.3f, size.height * 0.7f)
            lineTo(1f, size.height * 0.9f)
            close()
        }
        drawPath(cursor, Color.Black.copy(alpha = 0.75f))
        drawPath(cursor, Color.White)
    }
}

@Composable
private fun SetupFakeStatusLine(settings: AppSettings, expanded: Boolean) {
    val enabledItems = StreamStatusItem.entries.filter { it.enabledIn(settings) }
    val values = enabledItems.mapNotNull { item -> item.previewValueRes?.let { stringResource(it) } }
    val keyboardEnabled = StreamStatusItem.Keyboard.enabledIn(settings)
    val detailed = settings.streamStatsStyle == StreamStatsStyle.Detailed
    Surface(
        modifier = Modifier
            .padding(if (expanded) 14.dp else 8.dp)
            .widthIn(max = if (detailed) 300.dp else if (expanded) 720.dp else 360.dp),
        shape = RoundedCornerShape(if (detailed) OpenNowRadius.lg else OpenNowRadius.full),
        color = Color.Black.copy(alpha = 0.58f),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.18f)),
    ) {
        Row(
            Modifier.padding(horizontal = 12.dp, vertical = if (detailed) 8.dp else 6.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = values.takeIf { it.isNotEmpty() }?.joinToString(if (detailed) "\n" else "  •  ")
                    ?: stringResource(R.string.setup_play_status_empty),
                modifier = Modifier.weight(1f, fill = false),
                color = Color.White,
                style = MaterialTheme.typography.labelSmall,
                maxLines = if (detailed) 5 else 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (keyboardEnabled) {
                Icon(
                    painter = painterResource(R.drawable.ic_keyboard),
                    contentDescription = stringResource(R.string.stream_panel_cd_keyboard),
                    tint = Color.White,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
    }
}

@Composable
private fun SetupPracticeMenu(expanded: Boolean, onClose: () -> Unit, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.widthIn(min = if (expanded) 260.dp else 170.dp, max = 320.dp),
        shape = RoundedCornerShape(OpenNowRadius.lg),
        color = Color(0xFF101B21).copy(alpha = 0.96f),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.18f)),
    ) {
        Column(
            Modifier.padding(if (expanded) 16.dp else 10.dp),
            verticalArrangement = Arrangement.spacedBy(if (expanded) 10.dp else 5.dp),
        ) {
            Text(
                stringResource(R.string.stream_panel_title),
                color = Color.White,
                style = if (expanded) MaterialTheme.typography.titleMedium else MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
            )
            listOf(
                stringResource(R.string.setup_play_fake_status_line),
                stringResource(R.string.setup_play_fake_input),
                stringResource(R.string.setup_play_fake_quality),
            ).forEach { label ->
                Surface(
                    modifier = Modifier.fillMaxWidth().clickable(onClick = onClose),
                    shape = RoundedCornerShape(OpenNowRadius.md),
                    color = Color.White.copy(alpha = 0.07f),
                ) {
                    Text(
                        label,
                        modifier = Modifier.padding(horizontal = 10.dp, vertical = if (expanded) 9.dp else 5.dp),
                        color = Color.White.copy(alpha = 0.86f),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }
    }
}

@Composable
private fun SetupPracticeKeyboard(expanded: Boolean, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(OpenNowRadius.md),
        color = Color.Black.copy(alpha = 0.72f),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.16f)),
    ) {
        Row(
            Modifier.padding(horizontal = if (expanded) 12.dp else 8.dp, vertical = 7.dp),
            horizontalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            (if (expanded) listOf("W", "A", "S", "D", "SPACE", "ENTER") else listOf("W", "A", "S", "D")).forEach { key ->
                Text(
                    key,
                    modifier = Modifier
                        .clip(RoundedCornerShape(5.dp))
                        .background(Color.White.copy(alpha = 0.12f))
                        .padding(horizontal = if (key.length > 1) 8.dp else 6.dp, vertical = 4.dp),
                    color = Color.White,
                    style = MaterialTheme.typography.labelSmall,
                    fontWeight = FontWeight.Bold,
                )
            }
        }
    }
}

@Composable
private fun SetupPracticeButton(
    label: String,
    compact: Boolean,
    emphasized: Boolean = false,
    onClick: () -> Unit,
) {
    Surface(
        modifier = Modifier
            .clip(RoundedCornerShape(OpenNowRadius.full))
            .clickable(onClick = onClick),
        shape = RoundedCornerShape(OpenNowRadius.full),
        color = if (emphasized) MaterialTheme.colorScheme.primary else Color.Black.copy(alpha = 0.62f),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.22f)),
    ) {
        Text(
            label,
            modifier = Modifier.padding(
                horizontal = if (compact) 8.dp else 13.dp,
                vertical = if (compact) 5.dp else 8.dp,
            ),
            color = if (emphasized) MaterialTheme.colorScheme.onPrimary else Color.White,
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.Bold,
            maxLines = 1,
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SetupStreamStatusItems(
    settings: AppSettings,
    onSettingsChange: (AppSettings) -> Unit,
) {
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val columns = when {
            maxWidth >= 720.dp -> 4
            maxWidth >= 480.dp -> 3
            else -> 2
        }
        val gap = 8.dp
        val itemWidth = (maxWidth - gap * (columns - 1)) / columns.toFloat()
        FlowRow(
            modifier = Modifier.fillMaxWidth(),
            maxItemsInEachRow = columns,
            horizontalArrangement = Arrangement.spacedBy(gap),
            verticalArrangement = Arrangement.spacedBy(gap),
        ) {
            StreamStatusItem.entries.forEach { item ->
                val enabled = item.enabledIn(settings)
                Surface(
                    modifier = Modifier
                        .width(itemWidth)
                        .clip(RoundedCornerShape(OpenNowRadius.md))
                        .clickable { onSettingsChange(item.setEnabled(settings, !enabled)) },
                    shape = RoundedCornerShape(OpenNowRadius.md),
                    color = if (enabled) {
                        MaterialTheme.colorScheme.primary.copy(alpha = 0.16f)
                    } else {
                        MaterialTheme.colorScheme.surfaceVariant
                    },
                    border = BorderStroke(
                        1.dp,
                        if (enabled) MaterialTheme.colorScheme.primary
                        else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.2f),
                    ),
                ) {
                    Row(
                        Modifier.padding(horizontal = 10.dp, vertical = 9.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(
                            Modifier
                                .size(8.dp)
                                .clip(CircleShape)
                                .background(
                                    if (enabled) MaterialTheme.colorScheme.primary
                                    else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.34f),
                                ),
                        )
                        Text(
                            stringResource(item.labelRes),
                            color = MaterialTheme.colorScheme.onSurface,
                            style = MaterialTheme.typography.labelMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun setupTouchMouseChoiceLabel(choice: SetupTouchMouseChoice): String =
    stringResource(
        when (choice) {
            SetupTouchMouseChoice.Direct -> R.string.setup_play_touch_direct
            SetupTouchMouseChoice.Trackpad -> R.string.setup_play_touch_trackpad
            SetupTouchMouseChoice.Off -> R.string.setup_play_touch_off
        },
    )

/**
 * The membership tier, stated plainly, above the quality choices.
 *
 * Without it a Free account sees 1080p as the top option and reads it as OpenNOW deciding the
 * device cannot do better. Naming the plan and its ceiling makes the cap attributable.
 */
@Composable
private fun SetupMembershipCard(entitlements: StreamPlanEntitlements) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.lg),
        color = MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Column(
            Modifier.fillMaxWidth().padding(OpenNowSpacing.md),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                stringResource(R.string.setup_streaming_membership, entitlements.planLabel),
                color = MaterialTheme.colorScheme.onSurface,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                stringResource(
                    R.string.setup_streaming_membership_ceiling,
                    entitlements.maxResolutionLabel,
                    entitlements.maxFps,
                ),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
            if (entitlements.cappedBelowTopTier) {
                Text(
                    stringResource(R.string.setup_streaming_membership_upgrade),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}

/**
 * Resolution, frame rate, and bitrate, edited here rather than in Settings > Stream.
 *
 * Options above the plan stay listed and disabled with the tier that unlocks them, matching how
 * Settings presents them — hiding them would leave the ceiling unexplained all over again.
 */
@Composable
private fun SetupCustomStreamControls(
    state: OpenNowUiState,
    viewModel: OpenNowViewModel,
    entitlements: StreamPlanEntitlements,
    fallbackMembershipTier: String?,
) {
    val stream = state.settings.stream
    val resolutionChoices = streamResolutionChoicesForAspect(stream.aspectRatio).ifEmpty {
        streamResolutionChoicesForAspect("16:9")
    }
    val selectedResolution = normalizeStreamResolutionForAspectAndPlan(
        stream.resolution,
        stream.aspectRatio,
        state.subscriptionInfo,
        fallbackMembershipTier,
    )
    val codecChoices = androidCodecChoicePresentation(
        stream = stream,
        codecReport = state.codecReport,
        comingSoonLabel = stringResource(R.string.option_coming_soon),
        unavailableLabel = stringResource(R.string.common_unavailable),
    )
    Column(verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm)) {
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
            selectedLabel = resolutionChoices.firstOrNull { it.value == selectedResolution }?.label
                ?: selectedResolution,
        ) { value ->
            viewModel.updateStreamSettings { it.copy(resolution = value) }
        }
        ChoiceMenuRow(
            label = stringResource(R.string.settings_codec),
            options = codecChoices.options,
            selectedLabel = codecChoices.selectedLabel,
            description = stringResource(R.string.settings_codec_desc),
        ) { value ->
            viewModel.updateStreamSettings {
                it.copy(codec = VideoCodec.valueOf(value)).withCodecColorCompatibility()
            }
        }
        NumberSlider(
            label = stringResource(R.string.settings_fps),
            value = stream.fps.coerceAtMost(entitlements.maxFps).toFloat(),
            min = 30f,
            max = entitlements.maxFps.toFloat(),
            step = 30f,
            unit = "FPS",
        ) { value ->
            val fps = value.roundToInt().coerceIn(30, entitlements.maxFps)
            viewModel.updateStreamSettings { it.copy(fps = fps) }
        }
        NumberSlider(
            label = stringResource(R.string.settings_bitrate),
            value = stream.maxBitrateMbps.toFloat(),
            min = 1f,
            max = 150f,
            step = 1f,
            descriptionProvider = { mbps -> streamBitrateUsageEstimate(mbps) },
        ) { value ->
            viewModel.updateStreamSettings { it.copy(maxBitrateMbps = value.roundToInt()) }
        }
    }
}

@Composable
private fun SetupFeedbackStep(settings: AppSettings, onSettingsChange: (AppSettings) -> Unit) {
    Column(
        Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
    ) {
        SetupPointsCard(
            title = stringResource(R.string.setup_feedback_reporter_title),
            points = listOf(
                stringResource(R.string.setup_feedback_reporter_where),
                stringResource(R.string.setup_feedback_reporter_preflight),
                stringResource(R.string.setup_feedback_reporter_contents),
            ),
        )
        SettingSwitch(
            label = stringResource(R.string.setup_feedback_session_report),
            checked = settings.showSessionReportAfterStream,
            description = stringResource(R.string.setup_feedback_session_report_desc),
        ) {
            onSettingsChange(settings.copy(showSessionReportAfterStream = it))
        }
        DiscordCommunityLink(
            summary = stringResource(R.string.discord_community_bug_report_summary),
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        )
    }
}

@Composable
private fun SetupReadyStep(settings: AppSettings, tvProfile: Boolean) {
    val backgroundValue = when {
        appBackgroundChoiceFor(settings) == AppBackgroundChoice.Default ->
            stringResource(R.string.setup_background_default)
        appBackgroundChoiceFor(settings) == AppBackgroundChoice.Nothing ->
            stringResource(R.string.setup_background_nothing)
        appBackgroundChoiceFor(settings) == AppBackgroundChoice.SystemWallpaper ->
            stringResource(R.string.settings_background_system_wallpaper)
        !settings.nerdCatalogBackgroundUri.isNullOrBlank() ->
            stringResource(R.string.settings_catalog_background_image_custom)
        else -> catalogBackgroundPresetLabel(settings.catalogBackgroundPreset)
    }
    ControlSection(stringResource(R.string.setup_summary_title)) {
        SetupSummaryRow(stringResource(R.string.setup_background), backgroundValue)
        SetupSummaryRow(stringResource(R.string.setup_appearance_accent), uiAccentLabel(settings.uiAccent))
        SetupSummaryRow(
            stringResource(R.string.setup_summary_streaming),
            settings.stream.recommendationSummary(),
        )
        if (!tvProfile) {
            SetupSummaryRow(
                stringResource(R.string.setup_summary_touch),
                setupTouchMouseChoiceLabel(setupTouchMouseChoiceFor(settings)),
            )
        }
        SetupSummaryRow(
            stringResource(R.string.setup_summary_status),
            if (settings.showStatsOnLaunch) settings.streamStatsPosition.label
            else stringResource(R.string.setup_summary_off),
        )
    }
}

@Composable
private fun SetupSectionLabel(title: String, value: String) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
        verticalAlignment = Alignment.Bottom,
    ) {
        Text(
            title,
            color = MaterialTheme.colorScheme.onBackground,
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.SemiBold,
        )
        Text(
            value,
            Modifier.weight(1f),
            color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.72f),
            style = MaterialTheme.typography.bodySmall,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** A picture-sized, D-pad focusable option. Selection is the accent ring; focus is the white one. */
@Composable
private fun SetupTile(
    selected: Boolean,
    onClick: () -> Unit,
    width: Dp = SetupTileMaxWidth,
    content: @Composable BoxScope.() -> Unit,
) {
    val controllerFocusEnabled = LocalControllerFocusEnabled.current
    var focused by remember { mutableStateOf(false) }
    val showFocusRing = focused && controllerFocusEnabled
    val shape = RoundedCornerShape(OpenNowRadius.md)
    Box(
        Modifier
            .size(width = width, height = SetupTileHeight)
            .border(
                width = when {
                    showFocusRing -> 3.dp
                    selected -> 2.dp
                    else -> 1.dp
                },
                color = when {
                    showFocusRing -> Color.White
                    selected -> MaterialTheme.colorScheme.primary
                    else -> MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.28f)
                },
                shape = shape,
            )
            .clip(shape)
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
        content = content,
    )
}

@Composable
private fun SetupAccentSwatch(accent: UiAccent, selected: Boolean, onClick: () -> Unit) {
    val controllerFocusEnabled = LocalControllerFocusEnabled.current
    var focused by remember { mutableStateOf(false) }
    val showFocusRing = focused && controllerFocusEnabled
    val ringColor = when {
        showFocusRing -> Color.White
        selected -> MaterialTheme.colorScheme.onBackground
        else -> Color.Transparent
    }
    Box(
        Modifier
            .size(SetupSwatchTarget)
            .border(if (ringColor == Color.Transparent) 0.dp else 2.dp, ringColor, CircleShape)
            .clip(CircleShape)
            .onFocusChanged { focused = it.isFocused || it.hasFocus }
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(SetupSwatchDot).clip(CircleShape).background(accent.color))
    }
}

/** A settings-style row whose trailing slot is a radio button — one of several exclusive options. */
@Composable
private fun SetupChoiceRow(label: String, value: String, selected: Boolean, onSelect: () -> Unit) {
    val style = controlRowStyle()
    ControlRow(onClick = onSelect) {
        ControlRowLabels(
            label = label,
            value = value,
            expandedDescription = null,
            enabled = true,
            style = style,
        )
        RadioButton(selected = selected, onClick = onSelect)
    }
}

@Composable
private fun SetupSummaryRow(label: String, value: String) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
        verticalAlignment = Alignment.Top,
    ) {
        Text(
            label,
            Modifier.weight(1f),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            style = MaterialTheme.typography.bodyMedium,
        )
        Text(
            value,
            Modifier.weight(1f),
            color = MaterialTheme.colorScheme.onSurface,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.SemiBold,
        )
    }
}

@Composable
private fun SetupPointsCard(title: String, points: List<String>) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(OpenNowRadius.lg),
        color = MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Column(
            Modifier.fillMaxWidth().padding(OpenNowSpacing.md),
            verticalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
        ) {
            Text(
                title,
                color = MaterialTheme.colorScheme.onSurface,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
            )
            points.forEach { point ->
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(OpenNowSpacing.sm),
                    verticalAlignment = Alignment.Top,
                ) {
                    Box(
                        Modifier
                            .padding(top = 7.dp)
                            .size(5.dp)
                            .clip(CircleShape)
                            .background(MaterialTheme.colorScheme.primary),
                    )
                    Text(
                        point,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        }
    }
}

private val SetupContentMaxWidth = 720.dp
private val SetupTileHeight = 68.dp
private val SetupTileMinWidth = 96.dp
private val SetupTileMaxWidth = 148.dp
private val SetupSwatchTarget = 40.dp
private val SetupSwatchDot = 28.dp

/**
 * Tiles per screen width. Fractional on purpose: 2.6 leaves most of a third tile showing, which is
 * what tells the user the row scrolls.
 */
private const val SETUP_TILE_PEEK_COUNT = 2.6f
private val SetupPeekFadeWidth = 36.dp
private val SetupPeekFadeColor = Color.Black.copy(alpha = 0.55f)
private const val SETUP_PREVIEW_CARD_COUNT = 3

private val SetupStep.titleRes: Int
    get() = when (this) {
        SetupStep.Welcome -> R.string.app_name
        SetupStep.Appearance -> R.string.setup_appearance_title
        SetupStep.Streaming -> R.string.setup_streaming_title
        SetupStep.Play -> R.string.setup_play_title
        SetupStep.Ready -> R.string.setup_ready_title
    }

private val SetupStep.subtitleRes: Int
    get() = when (this) {
        SetupStep.Welcome -> R.string.setup_welcome_tagline
        SetupStep.Appearance -> R.string.setup_appearance_subtitle
        SetupStep.Streaming -> R.string.setup_streaming_subtitle
        SetupStep.Play -> R.string.setup_play_subtitle
        SetupStep.Ready -> R.string.setup_ready_subtitle
    }
