package dev.contextswitch.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.contextswitch.LogbookStore
import dev.contextswitch.ProjectRec
import dev.contextswitch.TagRec
import kotlinx.coroutines.delay
import java.time.Duration
import java.time.LocalDateTime
import java.time.OffsetDateTime
import java.time.temporal.ChronoUnit

@OptIn(ExperimentalLayoutApi::class, ExperimentalMaterial3Api::class)
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
    var switchAtOpen by remember { mutableStateOf(false) }
    var editingActive by remember { mutableStateOf(false) }
    var overflowSection by remember { mutableStateOf<OverflowSection?>(null) }
    val usage = remember(snap) { snap?.let { computeUsage(it.spans, it.projects, it.tags) } }
    val projects = usage?.projects.orEmpty()
    val tags = usage?.tags.orEmpty()
    val combos = usage?.combos.orEmpty()

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
            Card(onClick = { editingActive = true }, modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                        Text(projectName, style = MaterialTheme.typography.titleLarge, color = projectColor(active.projectId))
                        Icon(Icons.Filled.Edit, "Edit active span", Modifier.size(18.dp))
                    }
                    Text(fmtDuration(elapsed), style = MaterialTheme.typography.displaySmall)
                    val tagNames = active.tagIds.mapNotNull { tid -> snap?.tags?.firstOrNull { it.id == tid }?.name }
                    if (tagNames.isNotEmpty()) Text(tagNames.joinToString(" · ", prefix = "+"))
                    active.description?.lines()?.firstOrNull()?.takeIf { it.isNotBlank() }?.let { firstLine ->
                        Text(
                            firstLine,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }

        if (combos.isNotEmpty()) {
            SectionHeader("🕒 Recent")
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                maxLines = RECENT_MAX_ROWS,
                overflow = FlowRowOverflow.expandIndicator {
                    EllipsisChip { overflowSection = OverflowSection.RECENT }
                },
            ) {
                combos.forEach { c ->
                    ComboChip(
                        c,
                        selected = projectId == c.projectId && tagIds == c.tagIds,
                    ) {
                        projectId = c.projectId
                        tagIds = c.tagIds
                    }
                }
            }
        }

        SectionHeader("📁 Project")
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            maxLines = PROJECT_MAX_ROWS,
            overflow = FlowRowOverflow.expandIndicator {
                EllipsisChip { overflowSection = OverflowSection.PROJECTS }
            },
        ) {
            UnassignedChip(projectId == null) { projectId = null }
            projects.forEach { p ->
                ProjectChip(p, selected = projectId == p.id) { projectId = p.id }
            }
        }

        if (tags.isNotEmpty()) {
            SectionHeader("🏷️ Tags")
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                maxLines = TAG_MAX_ROWS,
                overflow = FlowRowOverflow.expandIndicator {
                    EllipsisChip { overflowSection = OverflowSection.TAGS }
                },
            ) {
                tags.forEach { t ->
                    TagChip(t, selected = t.id in tagIds) {
                        tagIds = if (t.id in tagIds) tagIds - t.id else tagIds + t.id
                    }
                }
            }
        }

        overflowSection?.let { section ->
            AlertDialog(
                onDismissRequest = { overflowSection = null },
                title = { Text(section.title) },
                text = {
                    FlowRow(
                        Modifier.verticalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        when (section) {
                            OverflowSection.RECENT -> combos.forEach { c ->
                                ComboChip(c, selected = projectId == c.projectId && tagIds == c.tagIds) {
                                    projectId = c.projectId
                                    tagIds = c.tagIds
                                }
                            }
                            OverflowSection.PROJECTS -> {
                                UnassignedChip(projectId == null) { projectId = null }
                                projects.forEach { p ->
                                    ProjectChip(p, selected = projectId == p.id) { projectId = p.id }
                                }
                            }
                            OverflowSection.TAGS -> tags.forEach { t ->
                                TagChip(t, selected = t.id in tagIds) {
                                    tagIds = if (t.id in tagIds) tagIds - t.id else tagIds + t.id
                                }
                            }
                        }
                    }
                },
                confirmButton = {
                    TextButton(onClick = { overflowSection = null }) { Text("Done") }
                },
            )
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
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = { store.switchTo(projectId, tagIds.toList()) }) { Text("Switch") }
                    OutlinedButton(onClick = { switchAtOpen = true }) { Text("Switch at…") }
                }
                TextButton(onClick = { store.cancel() }) { Text("Cancel") }
            }
            if (editingActive) {
                EditSpanDialog(
                    snap = snap,
                    span = active,
                    onDismiss = { editingActive = false },
                    onSave = { start, stop, pid, tids, description ->
                        store.editSpan(
                            active.id,
                            start,
                            stop,
                            tids,
                            projectEdit(active.projectId, pid),
                            description,
                        )
                        editingActive = false
                    },
                    onDelete = {
                        store.removeSpan(active.id)
                        editingActive = false
                    },
                )
            }
            if (switchAtOpen) {
                var at by remember { mutableStateOf(LocalDateTime.now().truncatedTo(ChronoUnit.MINUTES)) }
                val valid = isSwitchTimeValid(active.startedAt, at)
                AlertDialog(
                    onDismissRequest = { switchAtOpen = false },
                    title = { Text("Switch at…") },
                    text = {
                        DateTimeField(
                            label = "Switch time",
                            value = at,
                            error = if (valid) null else "Must not be before the current span started",
                            onChange = { at = it },
                        )
                    },
                    confirmButton = {
                        TextButton(enabled = valid, onClick = {
                            switchAtOpen = false
                            store.switchAt(resolveIso(null, at), projectId, tagIds.toList())
                        }) { Text("Switch") }
                    },
                    dismissButton = { TextButton(onClick = { switchAtOpen = false }) { Text("Cancel") } },
                )
            }
        }
    }
}

private const val RECENT_MAX_ROWS = 2
private const val PROJECT_MAX_ROWS = 2
private const val TAG_MAX_ROWS = 2

private enum class OverflowSection(val title: String) {
    RECENT("Recent"),
    PROJECTS("Project"),
    TAGS("Tags"),
}

@Composable
private fun SectionHeader(title: String) {
    Column {
        Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        HorizontalDivider(Modifier.padding(top = 4.dp), color = MaterialTheme.colorScheme.outlineVariant)
    }
}

@Composable
private fun UnassignedChip(selected: Boolean, onClick: () -> Unit) =
    FilterChip(selected = selected, onClick = onClick, label = { Text("unassigned") })

@Composable
private fun ProjectChip(project: ProjectRec, selected: Boolean, onClick: () -> Unit) =
    FilterChip(
        selected = selected,
        onClick = onClick,
        label = { Text(project.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        leadingIcon = { Box(Modifier.size(10.dp).background(projectColor(project.id), CircleShape)) },
    )

@Composable
private fun TagChip(tag: TagRec, selected: Boolean, onClick: () -> Unit) =
    FilterChip(
        selected = selected,
        onClick = onClick,
        label = { Text(tag.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
    )

/** One-tap chip for a (project, tag-set) combo: `project +tag1 +tag2`, tags capped at 3. */
@Composable
private fun ComboChip(combo: RecentCombo, selected: Boolean, onClick: () -> Unit) {
    val shown = combo.tagNames.take(3)
    val label = buildString {
        append(combo.projectName)
        shown.forEach { append(" +").append(it) }
        val rest = combo.tagNames.size - shown.size
        if (rest > 0) append(" +").append(rest)
    }
    FilterChip(
        selected = selected,
        onClick = onClick,
        label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        leadingIcon = { Box(Modifier.size(10.dp).background(projectColor(combo.projectId), CircleShape)) },
    )
}

@Composable
private fun EllipsisChip(onClick: () -> Unit) =
    FilterChip(selected = false, onClick = onClick, label = { Text("…") })
