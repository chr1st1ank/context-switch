package dev.contextswitch.ui

import dev.contextswitch.ProjectEdit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SpanDescriptionTest {

    @Test
    fun `clampDescription keeps short text unchanged`() {
        assertEquals("hello", clampDescription("hello"))
        assertEquals("", clampDescription(""))
    }

    @Test
    fun `clampDescription truncates at the limit`() {
        val long = "x".repeat(MAX_DESCRIPTION_CHARS + 50)
        assertEquals(MAX_DESCRIPTION_CHARS, clampDescription(long).length)
    }

    @Test
    fun `clampDescription never exceeds the limit in unicode points`() {
        // A supplementary-plane char is two UTF-16 units; clamping units
        // guarantees the core's char limit can never be exceeded.
        val emojis = "😀".repeat(MAX_DESCRIPTION_CHARS)
        val clamped = clampDescription(emojis)
        assertTrue(clamped.length <= MAX_DESCRIPTION_CHARS)
        assertTrue(clamped.codePointCount(0, clamped.length) <= MAX_DESCRIPTION_CHARS)
    }

    @Test
    fun `projectEdit keeps when unchanged`() {
        assertEquals(ProjectEdit.Keep, projectEdit("p1", "p1"))
        assertEquals(ProjectEdit.Keep, projectEdit(null, null))
    }

    @Test
    fun `projectEdit clears when next is null`() {
        assertEquals(ProjectEdit.Clear, projectEdit("p1", null))
    }

    @Test
    fun `projectEdit sets when next differs`() {
        assertEquals(ProjectEdit.Set("p2"), projectEdit("p1", "p2"))
        assertEquals(ProjectEdit.Set("p2"), projectEdit(null, "p2"))
    }
}
