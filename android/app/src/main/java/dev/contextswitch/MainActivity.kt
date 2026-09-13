package dev.contextswitch

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Timer
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.core.content.ContextCompat
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import dev.contextswitch.ui.LogScreen
import dev.contextswitch.ui.ManageScreen
import dev.contextswitch.ui.SettingsScreen
import dev.contextswitch.ui.TimerScreen

class MainActivity : ComponentActivity() {

    private val notifPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS)
            != PackageManager.PERMISSION_GRANTED
        ) {
            notifPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        setContent { App() }
    }
}

@Composable
fun App() {
    val store = (androidx.compose.ui.platform.LocalContext.current.applicationContext as CoswApp).logbook
    val nav = rememberNavController()
    val snapshot by store.snapshot.collectAsState()
    val error by store.error.collectAsState()

    LaunchedEffect(snapshot?.activeSpanId) {
        val active = snapshot?.let { s -> s.activeSpanId?.let { id -> s.spans.firstOrNull { it.id == id } } }
        if (active != null) {
            val label = active.projectId
                ?.let { pid -> snapshot?.projects?.firstOrNull { it.id == pid }?.name }
                ?: "(unassigned)"
            TimerService.start(
                nav.context,
                dev.contextswitch.ui.parseTime(active.startedAt).toInstant().toEpochMilli(),
                label,
            )
        } else {
            TimerService.stop(nav.context)
        }
    }

    MaterialTheme {
        Scaffold(
            bottomBar = {
                NavigationBar {
                    val backStack by nav.currentBackStackEntryAsState()
                    val current = backStack?.destination?.route
                    listOf(
                        Triple("timer", "Timer", Icons.Filled.Timer),
                        Triple("log", "Log", Icons.AutoMirrored.Filled.List),
                        Triple("manage", "Manage", Icons.Filled.Folder),
                        Triple("settings", "Settings", Icons.Filled.Settings),
                    ).forEach { (route, label, icon) ->
                        NavigationBarItem(
                            selected = current == route,
                            onClick = {
                                nav.navigate(route) {
                                    popUpTo("timer") { saveState = true }
                                    launchSingleTop = true
                                    restoreState = true
                                }
                            },
                            icon = { Icon(icon, label) },
                            label = { Text(label) },
                        )
                    }
                }
            },
        ) { padding ->
            Surface(Modifier.padding(padding)) {
                NavHost(nav, startDestination = "timer") {
                    composable("timer") { TimerScreen(store) }
                    composable("log") { LogScreen(store) }
                    composable("manage") { ManageScreen(store) }
                    composable("settings") { SettingsScreen(store) }
                }
                error?.let { msg ->
                    AlertDialog(
                        onDismissRequest = { store.error.value = null },
                        confirmButton = { TextButton(onClick = { store.error.value = null }) { Text("OK") } },
                        title = { Text("Storage error") },
                        text = { Text(msg) },
                    )
                }
            }
        }
    }
}
