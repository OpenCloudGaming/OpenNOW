package com.opencloudgaming.opennow

import android.content.Intent
import android.os.Bundle
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels

class MainActivity : ComponentActivity() {
    private val viewModel: OpenNowViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            OpenNowTheme {
                OpenNowApp(viewModel)
            }
        }
        viewModel.handleExternalLaunchIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        viewModel.handleExternalLaunchIntent(intent)
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        return NativeStreamInputRouter.dispatchKey(event) || super.dispatchKeyEvent(event)
    }

    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean {
        return NativeStreamInputRouter.dispatchMotion(event) || super.dispatchGenericMotionEvent(event)
    }
}
