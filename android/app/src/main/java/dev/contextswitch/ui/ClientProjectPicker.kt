package dev.contextswitch.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp
import dev.contextswitch.ClientProjectEdit
import dev.contextswitch.ClientProjectRec
import dev.contextswitch.ProjectRec
import dev.contextswitch.SpanRec

/** The tri-state client project change for `CoswStore.editSpan`; `next == null` inherits. */
fun clientProjectEdit(current: String?, next: String?): ClientProjectEdit = when {
    next == current -> ClientProjectEdit.Keep
    next == null -> ClientProjectEdit.Inherit
    else -> ClientProjectEdit.Set(next)
}

/** A span's own override, else its project's client project. */
internal fun effectiveClientProjectId(span: SpanRec, projectsById: Map<String, ProjectRec>): String? =
    span.clientProjectId ?: span.projectId?.let { projectsById[it]?.clientProjectId }

/**
 * Pickable client projects, most-used first. Usage is the number of spans each one
 * effectively covers. Archived entries are hidden unless they are [keep]'s selection,
 * so an existing choice stays visible.
 */
internal fun orderClientProjects(
    spans: List<SpanRec>,
    projects: List<ProjectRec>,
    clientProjects: List<ClientProjectRec>,
    keep: String? = null,
): List<ClientProjectRec> {
    val projectsById = projects.associateBy { it.id }
    val counts = HashMap<String, Int>()
    for (span in spans) {
        effectiveClientProjectId(span, projectsById)?.let { counts.merge(it, 1, Int::plus) }
    }
    return clientProjects
        .filter { !it.archived || it.id == keep }
        .sortedWith(
            compareByDescending<ClientProjectRec> { counts[it.id] ?: 0 }
                .thenBy(String.CASE_INSENSITIVE_ORDER) { it.name }
                .thenBy { it.id },
        )
}

/**
 * Choose a client project by tapping a chip. The first chip is "no value": [noneLabel]
 * says what that means in context (e.g. "Inherit (AP-11)" on a span).
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ClientProjectChips(
    selectedId: String?,
    options: List<ClientProjectRec>,
    noneLabel: String,
    onSelect: (String?) -> Unit,
) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(
            selected = selectedId == null,
            onClick = { onSelect(null) },
            label = { Text(noneLabel) },
        )
        options.forEach { c ->
            FilterChip(
                selected = selectedId == c.id,
                onClick = { onSelect(c.id) },
                label = { Text(if (c.archived) "${c.name} (archived)" else c.name) },
            )
        }
    }
}
