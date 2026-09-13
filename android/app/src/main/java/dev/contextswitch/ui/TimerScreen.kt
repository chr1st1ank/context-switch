package dev.contextswitch.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
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
            var elapsed by remember { mutableLongStateOf(0L) }
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
                    Text(projectName, style = MaterialTheme.typography.titleLarge, color = projectColor(active.projectId))
                    Text(fmtDuration(elapsed), style = MaterialTheme.typography.displaySmall)
                    val tagNames = active.tagIds.mapNotNull { tid -> snap?.tags?.firstOrNull { it.id == tid }?.name }
                    if (tagNames.isNotEmpty()) Text(tagNames.joinToString(" · ", prefix = "+"))
                }
            }
        }

        Column {
            Text("📁 Project", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
            HorizontalDivider(Modifier.padding(top = 4.dp), color = MaterialTheme.colorScheme.outlineVariant)
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilterChip(selected = projectId == null, onClick = { projectId = null }, label = { Text("unassigned") })
            projects.forEach { p ->
                FilterChip(
                    selected = projectId == p.id,
                    onClick = { projectId = p.id },
                    label = { Text(p.name) },
                    leadingIcon = { Box(Modifier.size(10.dp).background(projectColor(p.id), CircleShape)) },
                )
            }
        }

        if (tags.isNotEmpty()) {
            Column {
                Text("🏷️ Tags", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                HorizontalDivider(Modifier.padding(top = 4.dp), color = MaterialTheme.colorScheme.outlineVariant)
            }
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
        if (active == null) {
            Button(
                onClick = { store.start(projectId, tagIds.toList()) },
                modifier = Modifier.fillMaxWidth().height(64.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF2E7D32)),
            ) { Text("Start", style = MaterialTheme.typography.titleLarge) }
        } else {
            Button(
                onClick = { store.stop() },
                modifier = Modifier.fillMaxWidth().height(64.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFC62828)),
            ) { Text("Stop", style = MaterialTheme.typography.titleLarge) }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                OutlinedButton(onClick = { store.switchTo(projectId, tagIds.toList()) }) { Text("Switch") }
                TextButton(onClick = { store.cancel() }) { Text("Cancel") }
            }
        }
    }
}
