package dev.contextswitch.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.contextswitch.LogbookStore
import dev.contextswitch.SnapshotRec
import dev.contextswitch.SpanRec
import kotlinx.coroutines.launch
import java.time.Duration
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

private sealed interface LogItem {
    val key: String

    data class DayHeader(val date: LocalDate, val totalSeconds: Long) : LogItem {
        override val key get() = "day-$date"
    }

    data class Entry(val span: SpanRec) : LogItem {
        override val key get() = span.id
    }
}

private fun groupByDay(spans: List<SpanRec>): List<LogItem> = spans
    .sortedByDescending { it.startedAt }
    .groupBy { localDate(it.startedAt) }
    .flatMap { (date, daySpans) ->
        val total = daySpans.sumOf { s ->
            s.stoppedAt?.let { Duration.between(parseTime(s.startedAt), parseTime(it)).seconds } ?: 0L
        }
        listOf(LogItem.DayHeader(date, total)) + daySpans.map { LogItem.Entry(it) }
    }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LogScreen(store: LogbookStore) {
    val snap by store.snapshot.collectAsState()
    var editing by remember { mutableStateOf<SpanRec?>(null) }
    var adding by remember { mutableStateOf(false) }
    var pickingDate by remember { mutableStateOf(false) }

    val items = remember(snap) { groupByDay(snap?.spans.orEmpty()) }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()

    Box(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize(), state = listState, contentPadding = PaddingValues(8.dp)) {
            items(items, key = { it.key }) { item ->
                when (item) {
                    is LogItem.DayHeader -> DayHeaderRow(item)
                    is LogItem.Entry -> SpanRow(snap, item.span, Modifier.clickable { editing = item.span })
                }
            }
        }
        Column(
            Modifier.align(Alignment.BottomEnd).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            SmallFloatingActionButton(onClick = { pickingDate = true }) {
                Icon(Icons.Filled.DateRange, "Jump to date")
            }
            FloatingActionButton(onClick = { adding = true }) {
                Icon(Icons.Filled.Add, "Add past span")
            }
        }
    }

    if (pickingDate) {
        val pickerState = rememberDatePickerState()
        DatePickerDialog(
            onDismissRequest = { pickingDate = false },
            confirmButton = {
                TextButton(onClick = {
                    pickingDate = false
                    val millis = pickerState.selectedDateMillis ?: return@TextButton
                    val target = Instant.ofEpochMilli(millis).atZone(ZoneOffset.UTC).toLocalDate()
                    // Newest first: jump to the most recent day on or before the target.
                    val index = items.indexOfFirst { it is LogItem.DayHeader && !it.date.isAfter(target) }
                    scope.launch {
                        listState.animateScrollToItem(if (index >= 0) index else items.lastIndex)
                    }
                }) { Text("Jump") }
            },
            dismissButton = { TextButton(onClick = { pickingDate = false }) { Text("Cancel") } },
        ) {
            DatePicker(state = pickerState)
        }
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
private fun DayHeaderRow(header: LogItem.DayHeader) {
    val today = LocalDate.now()
    val label = when (header.date) {
        today -> "Today"
        today.minusDays(1) -> "Yesterday"
        else -> header.date.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM))
    }
    val weekday = header.date.format(DateTimeFormatter.ofPattern("EEEE"))
    Column(Modifier.fillMaxWidth().padding(top = 12.dp, bottom = 4.dp, start = 4.dp, end = 4.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(
                "$label · $weekday",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.weight(1f),
            )
            Text(
                fmtDuration(header.totalSeconds),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
            )
        }
        HorizontalDivider(Modifier.padding(top = 4.dp), color = MaterialTheme.colorScheme.outlineVariant)
    }
}

@Composable
private fun SpanRow(snap: SnapshotRec?, span: SpanRec, modifier: Modifier) {
    val project = span.projectId?.let { pid -> snap?.projects?.firstOrNull { it.id == pid }?.name } ?: "(unassigned)"
    val tagNames = span.tagIds.mapNotNull { tid -> snap?.tags?.firstOrNull { it.id == tid }?.name }
    val end = span.stoppedAt
    val duration = if (end != null) {
        fmtDuration(Duration.between(parseTime(span.startedAt), parseTime(end)).seconds)
    } else "running"
    ListItem(
        leadingContent = {
            Box(
                Modifier
                    .width(4.dp)
                    .height(44.dp)
                    .background(projectColor(span.projectId), RoundedCornerShape(2.dp)),
            )
        },
        headlineContent = { Text("$project${if (tagNames.isNotEmpty()) "  +" + tagNames.joinToString(" +") else ""}") },
        supportingContent = { Text("${fmtLocalTime(span.startedAt)} – ${if (end != null) fmtLocalTime(end) else "…"}") },
        trailingContent = { Text(duration) },
        modifier = modifier,
    )
}
