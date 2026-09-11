package com.opencloudgaming.opennow

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathOperation
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin

/**
 * The painting half of a touch skin.
 *
 * These composables know how to draw a control and nothing else — no gamepad, no pointer input, no
 * settings. That split is what lets the settings screen show a live preview of a skin without
 * standing up a stream, and it keeps [OpenNowTouchControls] about input rather than about pixels.
 */

/** A real blur needs API 31, so a skin's bloom is this many fading strokes instead. */
private const val GLOW_PASSES = 3

/** A d-pad is this many times its own arm across; the arms sit one arm-and-a-bit from the hub. */
private const val DPAD_BOX_PER_ARM = 3.1f

internal fun touchDpadBoxSize(arm: Dp): Dp = arm * DPAD_BOX_PER_ARM

// region shapes

private fun Path.addPolygon(
    center: Offset,
    radiusX: Float,
    radiusY: Float,
    sides: Int,
    startDegrees: Float,
) {
    for (index in 0 until sides) {
        val angle = ((startDegrees + 360f / sides * index) * PI / 180f).toFloat()
        val x = center.x + radiusX * cos(angle)
        val y = center.y + radiusY * sin(angle)
        if (index == 0) moveTo(x, y) else lineTo(x, y)
    }
    close()
}

/** [inset] pulls the outline in, so the same shape serves as fill, gloss clip, border and rim. */
private fun capPath(bounds: Size, form: TouchSkinForm, inset: Float): Path {
    val path = Path()
    val width = bounds.width - inset * 2f
    val height = bounds.height - inset * 2f
    if (width <= 0f || height <= 0f) return path
    val rect = Rect(inset, inset, inset + width, inset + height)
    when (form.capShape) {
        TouchCapShape.Circle -> path.addOval(rect)
        TouchCapShape.Rounded -> {
            val radius = min(width, height) / 2f * (form.capCornerPercent.coerceIn(0, 100) / 100f)
            path.addRoundRect(RoundRect(rect, CornerRadius(radius, radius)))
        }
        // Flat-topped, so a two-letter legend still has the widest part of the cap to sit in.
        TouchCapShape.Hexagon -> path.addPolygon(rect.center, width / 2f, height / 2f, sides = 6, startDegrees = 0f)
    }
    return path
}

private fun shoulderPath(bounds: Size, form: TouchSkinForm, inset: Float): Path {
    val path = Path()
    val width = bounds.width - inset * 2f
    val height = bounds.height - inset * 2f
    if (width <= 0f || height <= 0f) return path
    val rect = Rect(inset, inset, inset + width, inset + height)
    when (form.shoulderShape) {
        TouchShoulderShape.Pill -> {
            val radius = height / 2f
            path.addRoundRect(RoundRect(rect, CornerRadius(radius, radius)))
        }
        TouchShoulderShape.Slab -> {
            val radius = min(width, height) * 0.24f
            path.addRoundRect(RoundRect(rect, CornerRadius(radius, radius)))
        }
        TouchShoulderShape.Wedge -> {
            val cut = min(width * 0.24f, height * 0.7f)
            path.moveTo(rect.left + cut, rect.top)
            path.lineTo(rect.right - cut, rect.top)
            path.lineTo(rect.right, rect.center.y)
            path.lineTo(rect.right - cut, rect.bottom)
            path.lineTo(rect.left + cut, rect.bottom)
            path.lineTo(rect.left, rect.center.y)
            path.close()
        }
    }
    return path
}

// endregion

// region painters

/**
 * Paints one closed control — a cap, a shoulder, a d-pad key.
 *
 * [pathFor] is handed an inset rather than a finished path because the bloom, the fill, the gloss
 * clip and the rim are all the same outline at four different sizes.
 */
private fun DrawScope.drawTouchSurface(
    colors: TouchSkinColors,
    form: TouchSkinForm,
    pressed: Boolean,
    fill: Color = colors.fillFor(pressed),
    rim: Boolean = form.capRim,
    pathFor: (Float) -> Path,
) {
    val border = colors.borderFor(pressed)
    val borderWidth = colors.borderWidthFor(pressed).toPx()
    val outline = pathFor(borderWidth / 2f)

    val glow = form.glow.toPx()
    if (glow > 0f && border.alpha > 0f) {
        repeat(GLOW_PASSES) { pass ->
            drawPath(
                path = outline,
                color = border.copy(alpha = border.alpha * 0.22f / (pass + 1)),
                style = Stroke(width = borderWidth + glow * 2f * (pass + 1) / GLOW_PASSES),
            )
        }
    }

    if (fill.alpha > 0f) drawPath(pathFor(0f), fill)

    if (form.gloss > 0f) {
        // Light from above and a little shade underneath is the whole trick behind a domed cap.
        clipPath(pathFor(borderWidth)) {
            drawRect(
                brush = Brush.verticalGradient(
                    0f to colors.sheen(form.gloss),
                    0.5f to Color.Transparent,
                    1f to Color.Black.copy(alpha = (colors.opacity * form.gloss * 0.34f).coerceIn(0f, 1f)),
                ),
            )
        }
    }

    if (borderWidth > 0f && border.alpha > 0f) {
        drawPath(outline, border, style = Stroke(width = borderWidth))
    }

    if (rim) {
        drawPath(
            path = pathFor(borderWidth + size.minDimension * 0.10f),
            color = colors.sheen(0.28f),
            style = Stroke(width = (borderWidth * 0.7f).coerceAtLeast(1f)),
        )
    }
}

/** [rotationDegrees] is clockwise from "points up". */
private fun DrawScope.drawDirectionMark(
    center: Offset,
    rotationDegrees: Float,
    extent: Float,
    color: Color,
    style: TouchDpadArrow,
) {
    if (style == TouchDpadArrow.None || color.alpha <= 0f || extent <= 0f) return
    rotate(rotationDegrees, center) {
        when (style) {
            TouchDpadArrow.Triangle -> drawPath(
                path = Path().apply {
                    moveTo(center.x, center.y - extent)
                    lineTo(center.x + extent * 0.88f, center.y + extent * 0.6f)
                    lineTo(center.x - extent * 0.88f, center.y + extent * 0.6f)
                    close()
                },
                color = color,
            )
            TouchDpadArrow.Chevron -> drawPath(
                path = Path().apply {
                    moveTo(center.x - extent * 0.85f, center.y + extent * 0.45f)
                    lineTo(center.x, center.y - extent * 0.5f)
                    lineTo(center.x + extent * 0.85f, center.y + extent * 0.45f)
                },
                color = color,
                style = Stroke(
                    width = extent * 0.34f,
                    cap = StrokeCap.Round,
                    join = StrokeJoin.Round,
                ),
            )
            TouchDpadArrow.None -> Unit
        }
    }
}

private fun DrawScope.drawTouchDpad(
    colors: TouchSkinColors,
    form: TouchSkinForm,
    armPx: Float,
    up: Boolean,
    down: Boolean,
    left: Boolean,
    right: Boolean,
) {
    val width = size.width
    val height = size.height
    val center = Offset(width / 2f, height / 2f)
    val border = colors.border
    val borderWidth = colors.borderWidth.toPx()
    val armDistance = (min(width, height) - armPx) / 2f
    val markExtent = armPx * 0.2f
    // up, right, down, left — the order the rotations below step through.
    val pressedFlags = listOf(up, right, down, left)

    when (form.dpadShape) {
        TouchDpadShape.Cross -> {
            val corner = CornerRadius(8.dp.toPx(), 8.dp.toPx())
            fun crossPath(inset: Float): Path {
                val vertical = Path().apply {
                    addRoundRect(
                        RoundRect(
                            left = (width - armPx) / 2f + inset,
                            top = inset,
                            right = (width + armPx) / 2f - inset,
                            bottom = height - inset,
                            cornerRadius = corner,
                        ),
                    )
                }
                val horizontal = Path().apply {
                    addRoundRect(
                        RoundRect(
                            left = inset,
                            top = (height - armPx) / 2f + inset,
                            right = width - inset,
                            bottom = (height + armPx) / 2f - inset,
                            cornerRadius = corner,
                        ),
                    )
                }
                // Stroking two overlapping rectangles leaves their complete outlines visible at
                // the hub, which looks like a sharp square sitting on top of the d-pad. Union them
                // first so the classic skin has one continuous silhouette and one outer border.
                return Path.combine(PathOperation.Union, vertical, horizontal)
            }

            // Keep the stroke inside the canvas and reuse this one union for fill, clipping and
            // outline. D-pad presses only invalidate paint; they should not rebuild extra paths.
            val cross = crossPath(borderWidth / 2f)
            // Give each direction a disjoint quadrant. Rectangle halves overlap at the
            // hub on diagonals, and painting over the resting fill compounds its alpha.
            val corners = listOf(Offset.Zero, Offset(width, 0f), Offset(width, height), Offset(0f, height))
            val pressedPath = Path().apply {
                pressedFlags.forEachIndexed { index, pressed ->
                    if (pressed) {
                        moveTo(center.x, center.y)
                        lineTo(corners[index].x, corners[index].y)
                        val next = corners[(index + 1) % corners.size]
                        lineTo(next.x, next.y)
                        close()
                    }
                }
            }
            val resting = Path.combine(PathOperation.Difference, cross, pressedPath)
            val active = Path.combine(PathOperation.Intersect, cross, pressedPath)
            drawPath(resting, colors.dpadFill)
            drawPath(active, colors.pressedFill)
            drawPath(cross, border, style = Stroke(width = borderWidth))
        }

        TouchDpadShape.Segmented -> {
            val keyRadius = armPx / 2f * (form.dpadCornerPercent.coerceIn(0, 100) / 100f)
            pressedFlags.forEachIndexed { index, isPressed ->
                rotate(90f * index, center) {
                    val key = { inset: Float ->
                        Path().apply {
                            val rect = Rect(
                                center = Offset(center.x, center.y - armDistance),
                                radius = armPx / 2f - inset,
                            )
                            if (rect.width > 0f) {
                                addRoundRect(RoundRect(rect, CornerRadius(keyRadius, keyRadius)))
                            }
                        }
                    }
                    drawTouchSurface(
                        colors = colors,
                        form = form,
                        // Keep the four-key silhouette steady. Expanding one pressed outline (and
                        // its glow) into the center gap makes neighbouring directions collide.
                        pressed = false,
                        fill = if (isPressed) colors.pressedFill else colors.dpadFill,
                        rim = false,
                        pathFor = key,
                    )
                }
            }
        }

        TouchDpadShape.Disc -> {
            val radius = min(width, height) / 2f - borderWidth / 2f
            val pad = { inset: Float ->
                Path().apply {
                    val r = radius - inset + borderWidth / 2f
                    if (r > 0f) addOval(Rect(center = center, radius = r))
                }
            }
            drawTouchSurface(
                colors = colors,
                form = form,
                pressed = false,
                fill = colors.dpadFill,
                rim = false,
                pathFor = pad,
            )
            // A pressed direction lights its quadrant, which is exactly what the hit test measures.
            // Held inside the outline: a slice drawn out to it would repaint half the stroke and
            // leave a pressed disc with a heavier edge than every other control in the skin.
            val sliceRadius = radius - borderWidth / 2f
            pressedFlags.forEachIndexed { index, isPressed ->
                if (!isPressed) return@forEachIndexed
                drawArc(
                    color = colors.pressedFill,
                    startAngle = -135f + 90f * index,
                    sweepAngle = 90f,
                    useCenter = true,
                    topLeft = Offset(center.x - sliceRadius, center.y - sliceRadius),
                    size = Size(sliceRadius * 2f, sliceRadius * 2f),
                )
            }
        }

        TouchDpadShape.Blades -> {
            val outer = min(width, height) / 2f - borderWidth / 2f
            val inner = outer * 0.32f
            pressedFlags.forEachIndexed { index, isPressed ->
                rotate(90f * index, center) {
                    val blade = { inset: Float ->
                        Path().apply {
                            moveTo(center.x, center.y - outer + inset)
                            lineTo(center.x + outer * 0.44f - inset, center.y - inner)
                            lineTo(center.x - outer * 0.44f + inset, center.y - inner)
                            close()
                        }
                    }
                    drawTouchSurface(
                        colors = colors,
                        form = form,
                        // The fill and glyph carry directional feedback; a thicker pressed edge
                        // would visually run into the adjacent blades at the hub.
                        pressed = false,
                        fill = if (isPressed) colors.pressedFill else colors.dpadFill,
                        rim = false,
                        pathFor = blade,
                    )
                }
            }
        }
    }

    if (form.dpadArrow != TouchDpadArrow.None) {
        val markDistance = when (form.dpadShape) {
            TouchDpadShape.Disc -> min(width, height) / 2f * 0.64f
            else -> armDistance
        }
        pressedFlags.forEachIndexed { index, isPressed ->
            // Rotate the position around the pad hub as well as the arrow itself.
            // Rotating around the arrow's own center stacks all four into a star.
            rotate(90f * index, center) {
                drawDirectionMark(
                    center = Offset(center.x, center.y - markDistance),
                    rotationDegrees = 0f,
                    extent = markExtent,
                    color = colors.glyphFor(isPressed),
                    style = form.dpadArrow,
                )
            }
        }
    }
}

private fun DrawScope.drawTouchStick(
    colors: TouchSkinColors,
    form: TouchSkinForm,
    knobScale: Float,
    baseOffset: Offset,
    knobOffset: Offset,
) {
    val extent = min(size.width, size.height)
    val center = Offset(size.width / 2f, size.height / 2f) + baseOffset
    val hairline = 1.dp.toPx()
    val radius = extent / 2f - hairline
    val knobRadius = extent * knobScale / 2f
    val knobCenter = center + knobOffset
    val knobBorder = colors.stickKnobBorder

    fun drawKnobGloss() {
        if (form.gloss <= 0f) return
        drawCircle(
            brush = Brush.radialGradient(
                colors = listOf(colors.sheen(form.gloss + 0.14f), Color.Transparent),
                center = knobCenter - Offset(knobRadius * 0.3f, knobRadius * 0.36f),
                radius = knobRadius * 1.15f,
            ),
            radius = knobRadius,
            center = knobCenter,
        )
    }

    when (form.stickShape) {
        TouchStickShape.Ring -> {
            drawCircle(colors.stickTrack, radius, center, style = Stroke(width = hairline))
            drawCircle(colors.stickKnob, knobRadius, knobCenter)
            knobBorder?.let { drawCircle(it, knobRadius, knobCenter, style = Stroke(width = hairline)) }
        }

        TouchStickShape.Dish -> {
            drawCircle(colors.fill, radius, center)
            drawCircle(
                color = colors.stickTrack.copy(alpha = colors.stickTrack.alpha * 0.5f),
                radius = radius * 0.66f,
                center = center,
                style = Stroke(width = hairline),
            )
            drawCircle(colors.stickTrack, radius, center, style = Stroke(width = hairline * 1.5f))
            drawCircle(colors.stickKnob, knobRadius, knobCenter)
            drawKnobGloss()
            knobBorder?.let { drawCircle(it, knobRadius, knobCenter, style = Stroke(width = hairline * 1.5f)) }
        }

        TouchStickShape.Crosshair -> {
            // Drawn as arcs rather than a dashed stroke: path effects are not reliable on a
            // hardware-accelerated canvas, and this is on top of live video.
            val segments = 12
            repeat(segments) { index ->
                drawArc(
                    color = colors.stickTrack,
                    startAngle = 360f / segments * index,
                    sweepAngle = 360f / segments * 0.55f,
                    useCenter = false,
                    topLeft = Offset(center.x - radius, center.y - radius),
                    size = Size(radius * 2f, radius * 2f),
                    style = Stroke(width = hairline),
                )
            }
            repeat(4) { index ->
                rotate(90f * index, center) {
                    drawLine(
                        color = colors.stickTrack,
                        start = Offset(center.x, center.y - radius),
                        end = Offset(center.x, center.y - radius + extent * 0.09f),
                        strokeWidth = hairline * 1.5f,
                        cap = StrokeCap.Round,
                    )
                }
            }
            drawCircle(colors.stickKnob, knobRadius, knobCenter)
            drawCircle(
                color = knobBorder ?: colors.stickTrack,
                radius = knobRadius,
                center = knobCenter,
                style = Stroke(width = hairline * 1.5f),
            )
            repeat(2) { index ->
                rotate(90f * index, knobCenter) {
                    drawLine(
                        color = knobBorder ?: colors.stickTrack,
                        start = Offset(knobCenter.x - knobRadius * 0.55f, knobCenter.y),
                        end = Offset(knobCenter.x + knobRadius * 0.55f, knobCenter.y),
                        strokeWidth = hairline,
                    )
                }
            }
        }

        TouchStickShape.Hex -> {
            drawPath(
                path = Path().apply { addPolygon(center, radius, radius, sides = 6, startDegrees = 0f) },
                color = colors.stickTrack,
                style = Stroke(width = hairline * 1.5f),
            )
            val knob = Path().apply { addPolygon(knobCenter, knobRadius, knobRadius, sides = 6, startDegrees = 0f) }
            drawPath(knob, colors.stickKnob)
            knobBorder?.let { drawPath(knob, it, style = Stroke(width = hairline * 1.5f)) }
        }

        TouchStickShape.Gate -> {
            // A restrictor plate: the cap corners into the eight notches instead of sweeping freely.
            drawPath(
                path = Path().apply { addPolygon(center, radius, radius, sides = 8, startDegrees = 22.5f) },
                color = colors.stickTrack,
                style = Stroke(width = hairline * 2f),
            )
            drawPath(
                path = Path().apply {
                    addPolygon(center, radius * 0.55f, radius * 0.55f, sides = 8, startDegrees = 22.5f)
                },
                color = colors.stickTrack.copy(alpha = colors.stickTrack.alpha * 0.45f),
                style = Stroke(width = hairline),
            )
            val knobRect = Rect(center = knobCenter, radius = knobRadius)
            val knob = Path().apply {
                addRoundRect(RoundRect(knobRect, CornerRadius(knobRadius * 0.38f, knobRadius * 0.38f)))
            }
            drawPath(knob, colors.stickKnob)
            drawKnobGloss()
            knobBorder?.let { drawPath(knob, it, style = Stroke(width = hairline * 2f)) }
        }

        TouchStickShape.Ball -> {
            val trackWidth = hairline * 3f
            drawCircle(colors.stickTrack, radius - trackWidth / 2f, center, style = Stroke(width = trackWidth))
            // The shaft is what sells a ball top; it is hidden under the ball at rest.
            drawLine(
                color = colors.stickTrack,
                start = center,
                end = knobCenter,
                strokeWidth = knobRadius * 0.55f,
                cap = StrokeCap.Round,
            )
            drawCircle(colors.stickKnob, knobRadius, knobCenter)
            drawKnobGloss()
            knobBorder?.let { drawCircle(it, knobRadius, knobCenter, style = Stroke(width = hairline * 1.5f)) }
        }
    }
}

// endregion

// region composables

/** Blank caps are a supported look; the d-pad arrowheads are not optional, a bare cross is unusable. */
@Composable
private fun TouchButtonLabel(label: String, pressed: Boolean, sizeSp: Float) {
    if (!LocalTouchButtonLabels.current) return
    val colors = LocalTouchSkin.current
    val form = LocalTouchSkinForm.current
    Text(
        text = if (form.glyphUppercase) label.uppercase() else label,
        color = colors.glyphFor(pressed),
        fontFamily = form.glyphFamily,
        fontWeight = form.glyphWeight,
        fontSize = (sizeSp * form.glyphScale).sp,
        letterSpacing = form.glyphLetterSpacing,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

/** A face button, a thumb-stick click, or a start/select cap. Input belongs to the caller. */
@Composable
internal fun TouchCapFace(
    label: String,
    pressed: Boolean,
    diameter: Dp,
    modifier: Modifier = Modifier,
) {
    val colors = LocalTouchSkin.current
    val form = LocalTouchSkinForm.current
    val scale = if (pressed) form.pressScale else 1f
    Box(
        modifier
            .size(diameter)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .drawBehind {
                drawTouchSurface(colors, form, pressed) { inset -> capPath(size, form, inset) }
            },
        contentAlignment = Alignment.Center,
    ) {
        // The legend tracks the cap so the size sliders move the whole control, not just its outline.
        TouchButtonLabel(label, pressed, sizeSp = (diameter.value * 0.3f).coerceIn(7f, 20f))
    }
}

/** A trigger, a bumper, or a thumb-click pill. Input belongs to the caller. */
@Composable
internal fun TouchShoulderFace(
    label: String,
    pressed: Boolean,
    width: Dp,
    height: Dp,
    modifier: Modifier = Modifier,
) {
    val colors = LocalTouchSkin.current
    val form = LocalTouchSkinForm.current
    val scale = if (pressed) form.pressScale else 1f
    Box(
        modifier
            .width(width)
            .height(height)
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
            }
            .drawBehind {
                drawTouchSurface(colors, form, pressed, rim = false) { inset ->
                    shoulderPath(size, form, inset)
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        TouchButtonLabel(label, pressed, sizeSp = (height.value * 0.44f).coerceIn(7f, 18f))
    }
}

/** The whole d-pad, sized from one arm. Direction sensing belongs to the caller. */
@Composable
internal fun TouchDpadFace(
    arm: Dp,
    up: Boolean,
    down: Boolean,
    left: Boolean,
    right: Boolean,
    modifier: Modifier = Modifier,
) {
    val colors = LocalTouchSkin.current
    val form = LocalTouchSkinForm.current
    Canvas(modifier.size(touchDpadBoxSize(arm))) {
        drawTouchDpad(colors, form, arm.toPx(), up, down, left, right)
    }
}

/**
 * A stick's track and its travelling cap.
 *
 * [base] and [knob] are read inside the draw pass on purpose: a moving stick then only repaints,
 * never recomposes, which matters when it is riding on top of a live 60 fps video surface.
 */
@Composable
internal fun TouchStickFace(
    diameter: Dp,
    base: () -> Offset,
    knob: () -> Offset,
    modifier: Modifier = Modifier,
) {
    val colors = LocalTouchSkin.current
    val form = LocalTouchSkinForm.current
    val knobScale = (LocalTouchStickKnobScale.current * form.stickKnobScale).coerceIn(0.28f, 0.86f)
    Canvas(modifier.size(diameter)) {
        drawTouchStick(colors, form, knobScale, base(), knob())
    }
}

/**
 * A still life of a skin for the settings screen.
 *
 * Opacity is floored well below the slider's own range but above invisible: this is a picker, and a
 * skin faded out to a hint is one nobody can choose between.
 */
@Composable
internal fun TouchControllerSkinPreview(
    style: TouchControllerStyle,
    tint: ControllerThemeRgb?,
    opacity: Float,
    showLabels: Boolean,
    modifier: Modifier = Modifier,
) {
    val accent = remember(style, tint) {
        tint?.let { Color(it.r.coerceIn(0, 255), it.g.coerceIn(0, 255), it.b.coerceIn(0, 255)) }
            ?: defaultTouchSkinAccent(style)
    }
    val colors = remember(style, opacity, accent) {
        touchSkinColors(style, opacity.coerceAtLeast(0.6f), accent)
    }
    val form = remember(style) { touchSkinForm(style) }
    CompositionLocalProvider(
        LocalTouchSkin provides colors,
        LocalTouchSkinForm provides form,
        LocalTouchButtonLabels provides showLabels,
        LocalTouchStickKnobScale provides 0.44f,
    ) {
        Box(
            modifier
                .fillMaxWidth()
                .height(118.dp)
                .clip(RoundedCornerShape(18.dp))
                // A dark ground, because the real controller sits on top of a game, not on a sheet.
                .background(Brush.linearGradient(listOf(Color(0xff0d1219), Color(0xff1e2836))))
                .padding(horizontal = 14.dp),
            contentAlignment = Alignment.Center,
        ) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    TouchShoulderFace(label = "LB", pressed = false, width = 42.dp, height = 18.dp)
                    // A directional press in a still preview reads as a stuck input, so keep all
                    // four directions neutral. Live play still lights the direction being held.
                    TouchDpadFace(arm = 20.dp, up = false, down = false, left = false, right = false)
                }
                TouchStickFace(diameter = 62.dp, base = { Offset.Zero }, knob = { Offset.Zero })
                Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    TouchShoulderFace(label = "RB", pressed = false, width = 42.dp, height = 18.dp)
                    Box(Modifier.size(62.dp)) {
                        val cap = 20.dp
                        val spread = 21.dp
                        Box(Modifier.align(Alignment.Center).offset(y = -spread)) {
                            TouchCapFace("Y", pressed = false, diameter = cap)
                        }
                        Box(Modifier.align(Alignment.Center).offset(y = spread)) {
                            TouchCapFace("A", pressed = false, diameter = cap)
                        }
                        Box(Modifier.align(Alignment.Center).offset(x = -spread)) {
                            TouchCapFace("X", pressed = false, diameter = cap)
                        }
                        Box(Modifier.align(Alignment.Center).offset(x = spread)) {
                            // One cap held down, so the picker also shows what a press looks like.
                            TouchCapFace("B", pressed = true, diameter = cap)
                        }
                    }
                }
            }
        }
    }
}

// endregion
