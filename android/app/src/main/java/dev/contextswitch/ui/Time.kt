package dev.contextswitch.ui

import java.time.Duration
import java.time.OffsetDateTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter

private val localZone: ZoneId get() = ZoneId.systemDefault()
private val fmtDateTime: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm")

/** RFC 3339 (chrono `to_rfc3339`, e.g. `+00:00`) → [OffsetDateTime]. */
fun parseTime(s: String): OffsetDateTime = OffsetDateTime.parse(s)

fun fmtLocal(s: String): String = parseTime(s).atZoneSameInstant(localZone).format(fmtDateTime)

fun fmtLocalTime(s: String): String =
    parseTime(s).atZoneSameInstant(localZone).format(DateTimeFormatter.ofLocalizedTime(java.time.format.FormatStyle.SHORT))

fun localDate(s: String): java.time.LocalDate = parseTime(s).atZoneSameInstant(localZone).toLocalDate()

fun fmtDuration(seconds: Long): String {
    val d = Duration.ofSeconds(seconds.coerceAtLeast(0))
    val h = d.toHours()
    val m = d.toMinutes() % 60
    return if (h > 0) "${h}h ${m}m" else "${m}m ${d.seconds % 60}s"
}

fun toIsoUtc(local: OffsetDateTime): String = local.toInstant().toString()
