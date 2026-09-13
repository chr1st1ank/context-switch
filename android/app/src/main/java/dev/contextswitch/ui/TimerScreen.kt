package dev.contextswitch.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.contextswitch.LogbookStore
import kotlinx.coroutines.delay
import java.time.Duration
import java.time.OffsetDateTime

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun TimerScreen(store: LogbookStore) {
    val snap by store.snapshot.collectAsState()
    val configured by store.configured.collectAsState()

    if (!configured) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text("Configure your S3 backend in Settings first.")
        }
        return
    }

    val active = snap?.let { s -> s.activeSpanId?.let { id -> s.spans.firstOrNull { it.id == id } } }
    var projectId by remember { mutableStateOf<String?>(null) }
    var tagIds by remember { mutableStateOf(setOf<String>()) }
    val projects = snap?.projects?.filter { !it.archived }.orEmpty()
    val tags = snap?.tags?.filter { !it.archived }.orEmpty()

    Column(Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (active != null) {
            var elapsed by remember { mutableStateOf(0L) }
            LaunchedEffect(active.startedAt) {
                val start = parseTime(active.startedAt).toInstant()
                while (true) {
                    elapsed = Duration.between(start, OffsetDateTime.now().toInstant()).seconds
                    delay(1000)
                }
            }
            val projectName = active.projectId?.let { pid -> snap?.projects?.firstOrNull { it.id == pid }?.name } ?: "(unassigned)"
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(projectName, style = MaterialTheme.typography.titleLarge)
                    Text(fmtDuration(elapsed), style = MaterialTheme.typography.displaySmall)
                    val tagNames = active.tagIds.mapNotNull { tid -> snap?.tags?.firstOrNull { it.id == tid }?.name }
                    if (tagNames.isNotEmpty()) Text(tagNames.joinToString(" · ", prefix = "+"))
                }
            }
        }

        Text("Project", style = MaterialTheme.typography.labelLarge)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilterChip(selected = projectId == null, onClick = { projectId = null }, label = { Text("unassigned") })
            projects.forEach { p ->
                FilterChip(selected = projectId == p.id, onClick = { projectId = p.id }, label = { Text(p.name) })
            }
        }

        if (tags.isNotEmpty()) {
            Text("Tags", style = MaterialTheme.typography.labelLarge)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                tags.forEach { t ->
                    FilterChip(
                        selected = t.id in tagIds,
                        onClick = { tagIds = if (t.id in tagIds) tagIds - t.id else tagIds + t.id },
                        label = { Text(t.name) },
                    )
                }
            }
        }

        Spacer(Modifier.weight(1f))
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            if (active == null) {
                Button(onClick = { store.start(projectId, tagIds.toList()) }) { Text("Start") }
            } else {
                Button(onClick = { store.switchTo(projectId, tagIds.toList()) }) { Text("Switch") }
                OutlinedButton(onClick = { store.stop() }) { Text("Stop") }
                TextButton(onClick = { store.cancel() }) { Text("Cancel") }
            }
        }
    }
}
