package com.opencloudgaming.opennow

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.ui.controls.controlRowStyle

@Composable
internal fun SettingsDetailHeader(
    title: String,
    summary: String? = null,
    tvProfile: Boolean = false,
    controllerFamily: AndroidControllerFamily? = null,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    backFocusRequester: FocusRequester? = null,
    actions: @Composable RowScope.() -> Unit = {},
) {
    val controllerNavigationEnabled = LocalSettingsControllerNavigationEnabled.current
    val showHardwareBackHint = tvProfile || controllerNavigationEnabled
    var backFocused by remember { mutableStateOf(false) }
    val rowStyle = controlRowStyle()
    val space = LocalSpaceUiPresentation.current
    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.then(backFocusRequester?.let { Modifier.focusRequester(it) } ?: Modifier)) {
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
                                color = rowStyle.labelColor,
                                fontWeight = FontWeight.Bold,
                                style = MaterialTheme.typography.labelSmall,
                                modifier = Modifier.padding(end = 7.dp),
                            )
                        }
                    }
                } else {
                    Surface(
                        modifier = Modifier
                            .size(48.dp)
                            .border(1.dp, rowStyle.supportingColor.copy(alpha = 0.5f), CircleShape)
                            .clickable(onClick = onBack),
                        shape = CircleShape,
                        color = Color.Transparent,
                    ) {
                        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                            Icon(
                                painter = painterResource(R.drawable.ic_arrow_back),
                                contentDescription = stringResource(R.string.action_back),
                                tint = rowStyle.labelColor,
                                modifier = Modifier.size(21.dp),
                            )
                        }
                    }
                }
            }
            Spacer(Modifier.weight(1f))
            actions()
        }
        if (title.isNotBlank() || !summary.isNullOrBlank()) Column(Modifier.fillMaxWidth()) {
            Text(
                title,
                color = rowStyle.labelColor,
                style = if (space) MaterialTheme.typography.headlineLarge else MaterialTheme.typography.headlineSmall,
                fontWeight = if (space) FontWeight.Light else FontWeight.SemiBold,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            if (!summary.isNullOrBlank()) Text(
                summary,
                color = rowStyle.supportingColor,
                style = MaterialTheme.typography.bodyMedium,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

