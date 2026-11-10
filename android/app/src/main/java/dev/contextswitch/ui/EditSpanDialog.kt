package dev.contextswitch.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.contextswitch.SnapshotRec
import dev.contextswitch.SpanRec
import java.time.LocalDateTime
import java.time.ZoneId

/** Add or edit a span. Start and stop are picked with date/time pickers. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun EditSpanDialog(
    snap: SnapshotRec?,
    span: SpanRec?,
    onDismiss: () -> Unit,
    onSave: (
        startedIso: String?,
        stoppedIso: String?,
        projectId: String?,
        tagIds: List<String>,
        description: String,
        clientProjectId: String?,
    ) -> Unit,
    onDelete: (() -> Unit)?,
) {
    val zone = ZoneId.systemDefault()
    val running = span != null && span.stoppedAt == null
    var start by remember {
        mutableStateOf(span?.let { toLocalDateTime(it.startedAt, zone) } ?: LocalDateTime.now().minusHours(1))
    }
    var stop by remember {
        mutableStateOf(span?.stoppedAt?.let { toLocalDateTime(it, zone) }
            ?: if (span == null) LocalDateTime.now() else null)
    }
    var projectId by remember { mutableStateOf(span?.projectId) }
    var tagIds by remember { mutableStateOf(span?.tagIds?.toSet() ?: emptySet()) }
    var description by remember { mutableStateOf(span?.description ?: "") }
    var clientProjectId by remember { mutableStateOf(span?.clientProjectId) }
    val projects = snap?.projects?.filter { !it.archived }.orEmpty()
    val inheritedId = snap?.projects?.firstOrNull { it.id == projectId }?.clientProjectId
    val inheritedName = snap?.clientProjects?.firstOrNull { it.id == inheritedId }?.name
    val clientProjects = snap?.let {
        orderClientProjects(it.spans, it.projects, it.clientProjects, keep = clientProjectId)
    }.orEmpty()
    val tags = snap?.tags?.filter { !it.archived }.orEmpty()
    val stopError = if (stop != null && !isStopAfterStart(start, stop!!)) {
        "Must be after start"
    } else null
    val canSave = if (running) true else stop != null && stopError == null

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (span == null) "Add span" else "Edit span") },
        text = {
            Column(
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.verticalScroll(rememberScrollState()),
            ) {
                DateTimeField(label = "Start", value = start, onChange = { start = it })
                DateTimeField(
                    label = "Stop",
                    value = stop,
                    placeholder = "Running",
                    enabled = !running,
                    error = stopError,
                    onChange = { stop = it },
                )
                Text("Project", style = MaterialTheme.typography.labelMedium)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(selected = projectId == null, onClick = { projectId = null }, label = { Text("unassigned") })
                    projects.forEach { p ->
                        FilterChip(
                            selected = projectId == p.id,
                            onClick = { projectId = p.id },
                            label = { Text(p.name) },
                            leadingIcon = { Box(Modifier.size(10.dp).background(projectColor(p.id), RoundedCornerShape(5.dp))) },
                        )
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
                OutlinedTextField(
                    value = description,
                    onValueChange = { description = clampDescription(it) },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text("Description") },
                    supportingText = { Text("${description.length}/$MAX_DESCRIPTION_CHARS") },
                    maxLines = 4,
                )
                if (clientProjects.isNotEmpty() || inheritedName != null) {
                    Text("Client project", style = MaterialTheme.typography.labelMedium)
                    ClientProjectChips(
                        selectedId = clientProjectId,
                        options = clientProjects,
                        noneLabel = inheritedName?.let { "Inherit ($it)" } ?: "Inherit",
                        onSelect = { clientProjectId = it },
                    )
                }
            }
        },
        confirmButton = {
            TextButton(
                enabled = canSave,
                onClick = {
                    onSave(
                        resolveIso(span?.startedAt, start, zone),
                        if (running) null else stop?.let { resolveIso(span?.stoppedAt, it, zone) },
                        projectId,
                        tagIds.toList(),
                        description,
                        clientProjectId,
                    )
                },
            ) { Text("Save") }
        },
        dismissButton = {
            Row {
                if (onDelete != null) TextButton(onClick = onDelete) { Text("Delete") }
                TextButton(onClick = onDismiss) { Text("Cancel") }
            }
        },
    )
}
