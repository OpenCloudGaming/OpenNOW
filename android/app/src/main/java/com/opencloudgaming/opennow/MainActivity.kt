package com.opencloudgaming.opennow

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val viewModel: OpenNowViewModel by viewModels()
    private val queueStatusNotifier by lazy { AndroidQueueStatusNotifier(this) }
    private var notificationPermissionRequested = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            OpenNowApp(viewModel)
        }
        lifecycleScope.launch {
            viewModel.state.collect { state ->
                requestQueueNotificationPermissionIfNeeded(state)
                queueStatusNotifier.update(state)
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

    override fun onDestroy() {
        if (isFinishing) {
            queueStatusNotifier.cancel()
        }
        super.onDestroy()
    }

    @Suppress("DEPRECATION", "OVERRIDE_DEPRECATION")
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == 4210 && grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
            queueStatusNotifier.update(viewModel.state.value)
        }
    }

    private fun requestQueueNotificationPermissionIfNeeded(state: OpenNowUiState) {
        if (notificationPermissionRequested) return
        if (!shouldShowQueueLaunchStatus(state)) return
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        notificationPermissionRequested = true
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 4210)
    }
}
