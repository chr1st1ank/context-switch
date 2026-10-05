package dev.contextswitch.ui

import android.text.format.DateFormat
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

private val displayFormat: DateTimeFormatter =
    DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT)

/**
 * Read-only field that opens a date picker followed by a time picker.
 *
 * A null [value] displays [placeholder] (e.g. "Running") and disables picking
 * when [enabled] is false.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DateTimeField(
    label: String,
    value: LocalDateTime?,
    placeholder: String = "",
    enabled: Boolean = true,
    error: String? = null,
    onChange: (LocalDateTime) -> Unit,
) {
    var pickingDate by remember { mutableStateOf(false) }
    var pendingDate by remember { mutableStateOf<LocalDate?>(null) }
    val is24 = DateFormat.is24HourFormat(LocalContext.current)

    Box {
        OutlinedTextField(
            value = value?.format(displayFormat) ?: placeholder,
            onValueChange = {},
            readOnly = true,
            enabled = enabled,
            label = { Text(label) },
            isError = error != null,
            supportingText = error?.let { e -> { Text(e) } },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        if (enabled) {
            Box(Modifier.matchParentSize().clickable { pickingDate = true })
        }
    }

    if (pickingDate) {
        val initial = (value?.toLocalDate() ?: LocalDate.now())
            .atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli()
        val pickerState = rememberDatePickerState(initialSelectedDateMillis = initial)
        DatePickerDialog(
            onDismissRequest = { pickingDate = false },
            confirmButton = {
                TextButton(onClick = {
                    pickingDate = false
                    val millis = pickerState.selectedDateMillis ?: return@TextButton
                    pendingDate = Instant.ofEpochMilli(millis).atZone(ZoneOffset.UTC).toLocalDate()
                }) { Text("Next") }
            },
            dismissButton = { TextButton(onClick = { pickingDate = false }) { Text("Cancel") } },
        ) {
            DatePicker(state = pickerState)
        }
    }

    pendingDate?.let { date ->
        val initial = value?.toLocalTime() ?: LocalTime.now()
        val timeState = rememberTimePickerState(initial.hour, initial.minute, is24)
        AlertDialog(
            onDismissRequest = { pendingDate = null },
            confirmButton = {
                TextButton(onClick = {
                    onChange(LocalDateTime.of(date, LocalTime.of(timeState.hour, timeState.minute)))
                    pendingDate = null
                }) { Text("OK") }
            },
            dismissButton = { TextButton(onClick = { pendingDate = null }) { Text("Cancel") } },
            text = { TimePicker(state = timeState) },
        )
    }
}
