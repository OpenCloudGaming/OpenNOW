package com.opencloudgaming.opennow

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.DragIndicator
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/** Editor chrome can be moved away from any control, including controls in the middle. */
@Composable
internal fun TouchLayoutEditorPanel(onDone: () -> Unit, modifier: Modifier = Modifier) {
    BoxWithConstraints(modifier.safeDrawingPadding().padding(8.dp)) {
        // A new viewport or editing session starts with Done visible near the top.
        var offset by remember(maxWidth, maxHeight) { mutableStateOf(TouchOffset()) }
        Column(
            modifier = Modifier.align(Alignment.TopCenter)
                .draggableTouchOverlay(true, offset.x.dp, offset.y.dp, { x, y -> offset = TouchOffset(x, y) })
                .widthIn(max = 360.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Surface(
                shape = RoundedCornerShape(12.dp),
                color = Color.Black.copy(alpha = 0.8f),
                contentColor = Color.White,
            ) {
                Row(
                    Modifier.padding(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Icon(Icons.Rounded.DragIndicator,
                        contentDescription = stringResource(R.string.touch_drag_label),
                        modifier = Modifier.size(24.dp))
                    Text(stringResource(R.string.touch_layout_edit_hint),
                        modifier = Modifier.weight(1f),
                        style = MaterialTheme.typography.bodySmall)
                }
            }
            Button(
                onClick = onDone,
                shape = RoundedCornerShape(999.dp),
                contentPadding = PaddingValues(horizontal = 28.dp, vertical = 14.dp),
                elevation = ButtonDefaults.buttonElevation(defaultElevation = 8.dp),
            ) {
                Icon(Icons.Rounded.Check, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Text(stringResource(R.string.stream_panel_done),
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = FontWeight.SemiBold)
            }
        }
    }
}
