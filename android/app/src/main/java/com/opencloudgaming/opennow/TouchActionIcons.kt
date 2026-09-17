package com.opencloudgaming.opennow

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.unit.dp

/** Shared, cached artwork for the icon picker, button preview, and streaming controls. */
internal object TouchActionIcons {
    val Aim: ImageVector = actionIcon(
        name = "TouchAim",
        outline = "M9 5.7 A7 7 0 0 0 5.7 9 M5.7 15 A7 7 0 0 0 9 18.3 " +
            "M15 18.3 A7 7 0 0 0 18.3 15 M18.3 9 A7 7 0 0 0 15 5.7 " +
            "M12 2 V7 M12 17 V22 M2 12 H7 M17 12 H22 M10 12 H14 M12 10 V14",
    )

    val Crouch: ImageVector = actionIcon(
        name = "TouchCrouch",
        outline = "M12 8.5 L9 13.5 L15 15.5 L12 20.5 H18 " +
            "M12 8.5 L15 12 H20 M9 13.5 L5 17 H9",
        solid = "M16 5 A2 2 0 1 1 12 5 A2 2 0 1 1 16 5 Z",
    )

    val Jump: ImageVector = actionIcon(
        name = "TouchJump",
        outline = "M12 8 L11 13 M12 9 L8 8 L6 5 M12 9 L16 7 L18 3 " +
            "M11 13 L7 16 L5 14 M11 13 L15 16 L14 19 M8 22 H16",
        solid = "M14 4 A2 2 0 1 1 10 4 A2 2 0 1 1 14 4 Z",
    )

    val Reload: ImageVector = actionIcon(
        name = "TouchReload",
        outline = "M4.4 7.2 C6.6 3.7 11 2 15 3.6 L17 5 M17 1.8 V5 H13.8 " +
            "M19.6 16.8 C17.4 20.3 13 22 9 20.4 L7 19 M7 22.2 V19 H10.2 " +
            "M10 9 L12 6 L14 9 V17 H10 Z M10 14 H14",
    )

    val Accelerate: ImageVector = actionIcon(
        name = "TouchAccelerate",
        outline = "M12 21 V5 M6 11 L12 5 L18 11 M6 17 L12 11 L18 17",
    )

    val Brake: ImageVector = actionIcon(
        name = "TouchBrake",
        outline = "M7 3 H17 L19 21 H5 Z M9 8 H15 M8 12 H16 M7 16 H17",
    )

    val Handbrake: ImageVector = actionIcon(
        name = "TouchHandbrake",
        outline = "M4 20 H10 M6 19 L14 11 M14 11 L18 7 M14 11 L18 14",
    )

    val ShiftUp: ImageVector = actionIcon(
        name = "TouchShiftUp",
        outline = "M12 20 V4 M6 10 L12 4 L18 10 M7 20 H17",
    )

    val ShiftDown: ImageVector = actionIcon(
        name = "TouchShiftDown",
        outline = "M12 4 V20 M6 14 L12 20 L18 14 M7 4 H17",
    )

    val Horn: ImageVector = actionIcon(
        name = "TouchHorn",
        outline = "M4 9 H8 L16 5 V19 L8 15 H4 Z M18 9 C20 11 20 13 18 15",
    )

    val Dodge: ImageVector = actionIcon(
        name = "TouchDodge",
        outline = "M4 12 H16 M12 8 L16 12 L12 16 M9 5 L7 9 M9 19 L7 15",
        solid = "M10 3 A2 2 0 1 1 6 3 A2 2 0 1 1 10 3 Z",
    )

    val Knife: ImageVector = actionIcon(
        name = "TouchKnife",
        outline = "M4 20 L10 14 M9 15 L17 3 L21 7 L10 16 Z M4 20 L7 17",
    )

    val Door: ImageVector = actionIcon(
        name = "TouchDoor",
        outline = "M5 3 H18 V21 H5 Z M9 21 V6 L18 3 M14 12 H14.2",
    )

    private fun actionIcon(name: String, outline: String, solid: String? = null): ImageVector =
        ImageVector.Builder(name = name, defaultWidth = 24.dp, defaultHeight = 24.dp,
            viewportWidth = 24f, viewportHeight = 24f).apply {
            addPath(
                pathData = addPathNodes(outline),
                fill = null,
                stroke = SolidColor(Color.Black),
                strokeLineWidth = 2f,
                strokeLineCap = StrokeCap.Round,
                strokeLineJoin = StrokeJoin.Round,
            )
            if (solid != null) addPath(pathData = addPathNodes(solid), fill = SolidColor(Color.Black))
        }.build()
}
