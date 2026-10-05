package dev.contextswitch.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.LocalDateTime
import java.time.ZoneId

class SpanTimesTest {
    private val utc = ZoneId.of("UTC")
    private val berlin = ZoneId.of("Europe/Berlin")

    @Test
    fun `resolveIso preserves original when value unchanged`() {
        val original = "2025-03-10T09:15:42.123456+00:00"
        val picked = LocalDateTime.of(2025, 3, 10, 9, 15)
        assertEquals(original, resolveIso(original, picked, utc))
    }

    @Test
    fun `resolveIso converts picked time to UTC ISO`() {
        val picked = LocalDateTime.of(2025, 3, 10, 9, 30)
        assertEquals("2025-03-10T09:30:00Z", resolveIso(null, picked, utc))
    }

    @Test
    fun `resolveIso applies zone offset`() {
        val picked = LocalDateTime.of(2025, 6, 15, 12, 0) // CEST = UTC+2
        assertEquals("2025-06-15T10:00:00Z", resolveIso(null, picked, berlin))
    }

    @Test
    fun `resolveIso compares in the given zone`() {
        // 10:15 Berlin summer time is 08:15Z; editing in Berlin must not
        // overwrite the original's seconds.
        val original = "2025-06-15T08:15:30+00:00"
        val picked = LocalDateTime.of(2025, 6, 15, 10, 15)
        assertEquals(original, resolveIso(original, picked, berlin))
    }

    @Test
    fun `resolveIso resolves DST gap forward`() {
        // 2025-03-30 02:30 does not exist in Berlin; picks CEST 03:30 = 01:30Z.
        val picked = LocalDateTime.of(2025, 3, 30, 2, 30)
        assertEquals("2025-03-30T01:30:00Z", resolveIso(null, picked, berlin))
    }

    @Test
    fun `isStopAfterStart`() {
        val start = LocalDateTime.of(2025, 3, 10, 9, 0)
        assertTrue(isStopAfterStart(start, LocalDateTime.of(2025, 3, 10, 9, 1)))
        assertFalse(isStopAfterStart(start, LocalDateTime.of(2025, 3, 10, 9, 0)))
        assertFalse(isStopAfterStart(start, LocalDateTime.of(2025, 3, 10, 8, 59)))
    }

    @Test
    fun `toLocalDateTime converts to zone`() {
        assertEquals(
            LocalDateTime.of(2025, 6, 15, 10, 15, 30),
            toLocalDateTime("2025-06-15T08:15:30+00:00", berlin),
        )
    }
}
