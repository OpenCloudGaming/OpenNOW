package com.opencloudgaming.opennow.boosteroid

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.opencloudgaming.opennow.R

@Composable
internal fun BoosteroidLaunchStatus(launch: BoosteroidLaunch?, stopping: Boolean, onStop: () -> Unit) {
    BackHandler(enabled = !stopping, onBack = onStop)
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(
            Modifier.padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            launch?.title?.let { Text(it, style = MaterialTheme.typography.titleLarge) }
            CircularProgressIndicator(Modifier.size(30.dp))
            val position = launch?.session?.queuePosition
            Text(
                when {
                    stopping -> stringResource(R.string.boosteroid_ending)
                    position != null -> stringResource(R.string.boosteroid_queue_position, position)
                    else -> stringResource(R.string.boosteroid_launching)
                },
            )
            if (!stopping) TextButton(onClick = onStop) { Text(stringResource(R.string.action_cancel)) }
        }
    }
}

