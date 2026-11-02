package com.opencloudgaming.opennow

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.opencloudgaming.opennow.ui.theme.LocalReduceForegroundMotion

@Composable
internal fun SpaceLaunchScreen(state: OpenNowUiState, status: String, onMinimize: () -> Unit, onCancel: () -> Unit,
    queuePosition: Int? = null, connectingProgress: (() -> Float)? = null) {
    val context = LocalContext.current
    val requester = remember { FocusRequester() }
    val focused = LocalWindowInfo.current.isWindowFocused
    val minimizeEnabled = canMinimizeStreamLaunch(state.streamStatus, state.streamSession?.isReadyForStream() == true)
    LaunchedEffect(focused) { if (focused) requestFocusWithRetry(requester) }
    SpaceUiTheme {
        BoxWithConstraints(Modifier.fillMaxSize().testTag("space-launch").spaceSafeViewport()) {
            val compact = maxHeight < 500.dp
            val narrow = maxWidth < 500.dp
            SpaceSceneBackdrop(state.streamGame?.let { gameTvBannerImageUrl(context, it) })
            Column(Modifier.fillMaxSize().testTag("space-launch-content").spaceArrival(state.streamGame?.id)
                .padding(if (compact || narrow) 24.dp else 44.dp)) {
                OpenNowAppIcon(size = 32.dp)
                Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.CenterStart) {
                    Column(Modifier.fillMaxWidth(if (narrow) 1f else .78f)
                        .verticalScroll(rememberScrollState()).padding(vertical = 16.dp),
                        verticalArrangement = Arrangement.spacedBy(if (compact) 12.dp else 24.dp)) {
                        Text(state.streamGame?.title.orEmpty(), color = SpaceWhite, fontSize = if (compact || narrow) 34.sp else 54.sp,
                            lineHeight = if (compact || narrow) 40.sp else 62.sp, fontWeight = FontWeight.Light, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            if (state.error == null) SpaceLaunchStatusProgress(connectingProgress)
                            AnimatedQueueStatusText(status, queuePosition, compact,
                                modifier = Modifier.weight(1f, fill = false).testTag("space-queue-status")
                                    .spaceArrival(if (queuePosition == null) status else "queue")
                                    .clearAndSetSemantics { text = AnnotatedString(status) },
                                style = MaterialTheme.typography.titleLarge.copy(fontSize = 22.sp, fontWeight = FontWeight.Normal),
                                contentColor = SpaceWhite, reduceMotion = LocalReduceForegroundMotion.current)
                        }
                        state.error?.let { Text(it, color = MaterialTheme.colorScheme.error, maxLines = 3) }
                    }
                }
                Row(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    if (minimizeEnabled) SpaceControl(stringResource(R.string.action_minimize), Modifier.testTag("space-queue-minimize").focusRequester(requester),
                        filled = true, minHeight = 52.dp, onClick = onMinimize)
                    SpaceControl(stringResource(R.string.action_cancel),
                        modifier = Modifier.testTag("space-queue-cancel").then(if (!minimizeEnabled) Modifier.focusRequester(requester) else Modifier),
                        filled = true, minHeight = 52.dp, onClick = onCancel)
                }
            }
        }
    }
}

@Composable
private fun SpaceLaunchStatusProgress(connectingProgress: (() -> Float)?) {
    val modifier = Modifier.size(18.dp).testTag("space-launch-progress").clearAndSetSemantics { }
    if (connectingProgress != null) {
        CircularProgressIndicator(progress = connectingProgress, modifier = modifier, color = SpaceWhite,
            trackColor = SpaceWhite.copy(alpha = .12f), strokeWidth = 2.dp)
    } else if (LocalReduceForegroundMotion.current || !LocalWindowInfo.current.isWindowFocused) {
        CircularProgressIndicator(progress = { .72f }, modifier = modifier, color = SpaceWhite,
            trackColor = SpaceWhite.copy(alpha = .12f), strokeWidth = 2.dp)
    } else {
        CircularProgressIndicator(modifier = modifier, color = SpaceWhite,
            trackColor = SpaceWhite.copy(alpha = .12f), strokeWidth = 2.dp)
    }
}
