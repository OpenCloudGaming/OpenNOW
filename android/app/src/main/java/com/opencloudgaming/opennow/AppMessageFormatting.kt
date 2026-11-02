package com.opencloudgaming.opennow

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration

// Shared with published bugs. Markers may be combined (for example <red>**bold red**</red>).
// Unknown markup remains literal text; the app never interprets HTML.
private val messageToken = Regex("""\*\*([^*\n]+)\*\*|<red>([\s\S]*?)</red>|\[([^\]\n]+)\]\((https://[^\s)]+)\)""")

internal fun formattedAppMessageBody(body: String, linkColor: Color, redColor: Color): AnnotatedString =
    buildAnnotatedString { appendFormattedMessage(body, linkColor, redColor, 0) }

private fun AnnotatedString.Builder.appendFormattedMessage(body: String, linkColor: Color, redColor: Color, depth: Int) {
    if (depth >= 4) { append(body); return }
    var cursor = 0
    for (match in messageToken.findAll(body)) {
        append(body.substring(cursor, match.range.first))
        val start = length
        val bold = match.groups[1]?.value
        val red = match.groups[2]?.value
        val label = match.groups[3]?.value
        when {
            bold != null -> {
                appendFormattedMessage(bold, linkColor, redColor, depth + 1)
                addStyle(SpanStyle(fontWeight = FontWeight.Bold), start, length)
            }
            red != null -> {
                appendFormattedMessage(red, linkColor, redColor, depth + 1)
                addStyle(SpanStyle(color = redColor), start, length)
            }
            label != null -> {
                appendFormattedMessage(label, linkColor, redColor, depth + 1)
                addStyle(SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline), start, length)
                addLink(LinkAnnotation.Url(match.groups[4]!!.value), start, length)
            }
        }
        cursor = match.range.last + 1
    }
    append(body.substring(cursor))
}
