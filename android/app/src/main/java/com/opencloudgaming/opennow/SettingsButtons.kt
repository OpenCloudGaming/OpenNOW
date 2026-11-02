package com.opencloudgaming.opennow

import androidx.compose.foundation.hoverable
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.unit.dp

@Composable
internal fun SettingsButton(onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true,
    contentPadding: PaddingValues = ButtonDefaults.ContentPadding, content: @Composable RowScope.() -> Unit) =
    SettingsButtonSurface(false, onClick, modifier, enabled, contentPadding, content)

@Composable
internal fun SettingsOutlinedButton(onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true,
    contentPadding: PaddingValues = ButtonDefaults.ContentPadding, content: @Composable RowScope.() -> Unit) =
    SettingsButtonSurface(true, onClick, modifier, enabled, contentPadding, content)

/** Keep action semantics in Material; one scoped owner supplies the console treatment. */
@Composable
private fun SettingsButtonSurface(outlined: Boolean, onClick: () -> Unit, modifier: Modifier, enabled: Boolean,
    padding: PaddingValues, content: @Composable RowScope.() -> Unit) {
    val space = LocalSpaceUiPresentation.current
    var focused by remember { mutableStateOf(false) }
    val motion = if (space) rememberSpaceInteraction(focused, enabled = enabled) else null
    val presentation = if (motion != null) Modifier.heightIn(min = 52.dp).then(motion.modifier)
        .onFocusChanged { focused = it.isFocused }.hoverable(motion.source, enabled) else Modifier
    val shape = if (space) RoundedCornerShape(4.dp) else ButtonDefaults.shape
    if (outlined) {
        val surface = if (motion != null) Modifier.clip(shape).spaceAnimatedSurface(motion.emphasis, shape,
            SpaceWhite.copy(alpha = .035f), SpaceWhite.copy(alpha = .135f),
            SpaceWhite.copy(alpha = .15f), SpaceWhite.copy(alpha = .65f)) else Modifier
        androidx.compose.material3.OutlinedButton(onClick, modifier.then(presentation).then(surface), enabled,
            shape = shape,
            colors = if (space) ButtonDefaults.outlinedButtonColors(
                containerColor = Color.Transparent,
                contentColor = SpaceWhite) else ButtonDefaults.outlinedButtonColors(),
            border = if (space) null else ButtonDefaults.outlinedButtonBorder(enabled),
            contentPadding = padding, interactionSource = motion?.source, content = content)
    } else {
        androidx.compose.material3.Button(onClick, modifier.then(presentation), enabled,
            shape = shape,
            colors = if (space) ButtonDefaults.buttonColors(containerColor = SpaceWhite, contentColor = SpaceInk) else ButtonDefaults.buttonColors(),
            contentPadding = padding, interactionSource = motion?.source, content = content)
    }
}
