package dev.contextswitch.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.contextswitch.LogbookStore
import dev.contextswitch.SnapshotRec
import dev.contextswitch.SpanRec
import java.time.OffsetDateTime

@Composable
fun LogScreen(store: LogbookStore) {
    val snap by store.snapshot.collectAsState()
    var editing by remember { mutableStateOf<SpanRec?>(null) }
    var adding by remember { mutableStateOf(false) }

    Box(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(8.dp)) {
            items(snap?.spans.orEmpty(), key = { it.id }) { span ->
                SpanRow(snap, span, Modifier.clickable { editing = span })
                HorizontalDivider()
            }
        }
        FloatingActionButton(
            onClick = { adding = true },
            modifier = Modifier.align(androidx.compose.ui.Alignment.BottomEnd).padding(16.dp),
        ) { Icon(Icons.Filled.Add, "Add past span") }
    }

    editing?.let { span ->
        EditSpanDialog(
            snap = snap,
            span = span,
            onDismiss = { editing = null },
            onSave = { start, stop, projectId, tagIds ->
                store.editSpan(span.id, start, stop, tagIds)
                store.assignProject(span.id, projectId)
                editing = null
            },
            onDelete = {
                store.removeSpan(span.id)
                editing = null
            },
        )
    }
    if (adding) {
        EditSpanDialog(
            snap = snap,
            span = null,
            onDismiss = { adding = false },
            onSave = { start, stop, projectId, tagIds ->
                if (start != null && stop != null) store.addSpan(start, stop, projectId, tagIds)
                adding = false
            },
            onDelete = null,
        )
    }
}

@Composable
private fun SpanRow(snap: SnapshotRec?, span: SpanRec, modifier: Modifier) {
    val project = span.projectId?.let { pid -> snap?.projects?.firstOrNull { it.id == pid }?.name } ?: "(unassigned)"
    val tagNames = span.tagIds.mapNotNull { tid -> snap?.tags?.firstOrNull { it.id == tid }?.name }
    val end = span.stoppedAt
    val duration = if (end != null) {
        fmtDuration(java.time.Duration.between(parseTime(span.startedAt), parseTime(end)).seconds)
    } else "running"
    ListItem(
        headlineContent = { Text("$project${if (tagNames.isNotEmpty()) "  +" + tagNames.joinToString(" +") else ""}") },
        supportingContent = { Text("${fmtLocal(span.startedAt)} – ${if (end != null) fmtLocal(end) else "…"}") },
        trailingContent = { Text(duration) },
        modifier = modifier,
    )
}

/** Add or edit a span. Times are entered as local `yyyy-MM-dd HH:mm`. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun EditSpanDialog(
    snap: SnapshotRec?,
    span: SpanRec?,
    onDismiss: () -> Unit,
    onSave: (startedIso: String?, stoppedIso: String?, projectId: String?, tagIds: List<String>) -> Unit,
    onDelete: (() -> Unit)?,
) {
    fun toLocalInput(s: String?) = s?.let { parseTime(it).atZoneSameInstant(java.time.ZoneId.systemDefault()).format(java.time.format.DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm")) } ?: ""
    var start by remember { mutableStateOf(toLocalInput(span?.startedAt)) }
    var stop by remember { mutableStateOf(toLocalInput(span?.stoppedAt)) }
    var projectId by remember { mutableStateOf(span?.projectId) }
    var tagIds by remember { mutableStateOf(span?.tagIds?.toSet() ?: emptySet()) }
    val projects = snap?.projects?.filter { !it.archived }.orEmpty()
    val tags = snap?.tags?.filter { !it.archived }.orEmpty()

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (span == null) "Add span" else "Edit span") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(value = start, onValueChange = { start = it }, label = { Text("Start (yyyy-MM-dd HH:mm)") }, singleLine = true)
                OutlinedTextField(value = stop, onValueChange = { stop = it }, label = { Text("Stop (yyyy-MM-dd HH:mm)") }, singleLine = true)
                Text("Project", style = MaterialTheme.typography.labelMedium)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(selected = projectId == null, onClick = { projectId = null }, label = { Text("unassigned") })
                    projects.forEach { p ->
                        FilterChip(selected = projectId == p.id, onClick = { projectId = p.id }, label = { Text(p.name) })
                    }
                }
                if (tags.isNotEmpty()) {
                    Text("Tags", style = MaterialTheme.typography.labelMedium)
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
            }
        },
        confirmButton = {
            TextButton(onClick = {
                fun parse(input: String): String? = runCatching {
                    java.time.LocalDateTime.parse(input.trim(), java.time.format.DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm"))
                        .atZone(java.time.ZoneId.systemDefault())
                        .toOffsetDateTime()
                        .let { toIsoUtc(it) }
                }.getOrNull()
                onSave(parse(start), parse(stop), projectId, tagIds.toList())
            }) { Text("Save") }
        },
        dismissButton = {
            Row {
                if (onDelete != null) TextButton(onClick = onDelete) { Text("Delete") }
                TextButton(onClick = onDismiss) { Text("Cancel") }
            }
        },
    )
}
