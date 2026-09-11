package com.opencloudgaming.opennow

import android.os.Build
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import androidx.annotation.Keep
import androidx.annotation.RequiresApi

/**
 * Owns every reference to Android 8's pointer-capture API.
 *
 * API 25 verifies an Activity's field and method signatures before `onCreate()`. Keeping
 * [View.OnCapturedPointerListener] out of MainActivity lets Fire OS 6 load the app; this class is
 * reached only after the SDK check on Android 8 or newer.
 */
@Keep
@RequiresApi(Build.VERSION_CODES.O)
internal object AndroidPointerCapture {
    fun hasCapture(view: View): Boolean = view.hasPointerCapture()

    fun request(view: View) {
        view.requestPointerCapture()
    }

    fun release(view: View) {
        view.releasePointerCapture()
    }

    fun installRecursively(view: View, onMotion: (MotionEvent) -> Boolean) {
        val listener = View.OnCapturedPointerListener { _, event -> onMotion(event) }
        applyListenerRecursively(view, listener)
    }

    fun clearRecursively(view: View) {
        applyListenerRecursively(view, null)
    }

    fun configure(
        view: View,
        enabled: Boolean,
        onCaptureInput: () -> Unit,
        onMotion: (MotionEvent) -> Boolean,
    ) {
        if (!enabled) {
            clear(view)
            return
        }
        view.setOnCapturedPointerListener { _, event ->
            onCaptureInput()
            onMotion(event)
        }
        view.post {
            if (view.isAttachedToWindow && view.hasWindowFocus() && !view.hasPointerCapture()) {
                view.isFocusable = true
                view.isFocusableInTouchMode = true
                view.requestFocus()
                onCaptureInput()
                runCatching { view.requestPointerCapture() }
                    .onFailure { error ->
                        NativeInputDiagnostics.add(
                            "pointer capture request failed error=${error.javaClass.simpleName}",
                        )
                    }
            }
        }
    }

    fun clear(view: View) {
        view.setOnCapturedPointerListener(null)
        runCatching { view.releasePointerCapture() }
            .onFailure { error ->
                NativeInputDiagnostics.add(
                    "pointer capture release failed error=${error.javaClass.simpleName}",
                )
            }
    }

    private fun applyListenerRecursively(view: View, listener: View.OnCapturedPointerListener?) {
        view.setOnCapturedPointerListener(listener)
        if (view is ViewGroup) {
            for (index in 0 until view.childCount) {
                applyListenerRecursively(view.getChildAt(index), listener)
            }
        }
    }
}
