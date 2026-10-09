package dev.contextswitch.ui

import java.time.LocalDateTime
import java.time.ZoneId
import java.time.temporal.ChronoUnit

/** Local minute-precision value for display/editing of an RFC 3339 timestamp. */
fun toLocalDateTime(iso: String, zone: ZoneId = ZoneId.systemDefault()): LocalDateTime =
    parseTime(iso).atZoneSameInstant(zone).toLocalDateTime()

/**
 * ISO UTC string for a picked local date-time. When the picked value equals the
 * original at minute precision, the original string is returned unchanged so
 * seconds and sub-minute data are preserved.
 */
fun resolveIso(originalIso: String?, picked: LocalDateTime, zone: ZoneId = ZoneId.systemDefault()): String {
    if (originalIso != null &&
        toLocalDateTime(originalIso, zone).truncatedTo(ChronoUnit.MINUTES) ==
        picked.truncatedTo(ChronoUnit.MINUTES)
    ) {
        return originalIso
    }
    return toIsoUtc(picked.atZone(zone).toOffsetDateTime())
}

/** A switch may not happen before the active span started. */
fun isSwitchTimeValid(activeStartIso: String, picked: LocalDateTime, zone: ZoneId = ZoneId.systemDefault()): Boolean =
    !parseTime(resolveIso(null, picked, zone)).isBefore(parseTime(activeStartIso))

/** Valid span boundary: stop must be strictly after start. */
fun isStopAfterStart(start: LocalDateTime, stop: LocalDateTime): Boolean = stop.isAfter(start)
