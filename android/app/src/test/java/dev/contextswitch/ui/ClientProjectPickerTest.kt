package dev.contextswitch.ui

import dev.contextswitch.ClientProjectEdit
import dev.contextswitch.ClientProjectRec
import dev.contextswitch.ProjectRec
import dev.contextswitch.SpanRec
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ClientProjectPickerTest {
    private fun cp(id: String, name: String = id, archived: Boolean = false) =
        ClientProjectRec(id, name, archived)

    private fun project(id: String, clientProjectId: String? = null) =
        ProjectRec(id, id, null, clientProjectId, false)

    private fun span(id: String, projectId: String? = null, clientProjectId: String? = null) =
        SpanRec(id, "2025-06-15T10:00:00Z", "2025-06-15T11:00:00Z", projectId, emptyList(), null, clientProjectId, false)

    @Test
    fun `most used first counting inherited and overridden spans`() {
        val projects = listOf(project("p", "ap"))
        val spans = listOf(
            span("1", "p"), // inherits ap
            span("2", "p"), // inherits ap
            span("3", "p", "x"), // override wins over the project's value
            span("4", null, "x"),
            span("5", null, "x"),
            span("6", null, "x"),
        )
        val ordered = orderClientProjects(spans, projects, listOf(cp("ap", "AP-11"), cp("x", "X-1"), cp("unused", "A-0")))
        assertEquals(listOf("x", "ap", "unused"), ordered.map { it.id })
    }

    @Test
    fun `ties fall back to name order`() {
        val ordered = orderClientProjects(emptyList(), emptyList(), listOf(cp("2", "beta"), cp("1", "Alpha")))
        assertEquals(listOf("1", "2"), ordered.map { it.id })
    }

    @Test
    fun `archived hidden unless it is the current selection`() {
        val all = listOf(cp("a", "A"), cp("old", "Old", archived = true))
        assertEquals(listOf("a"), orderClientProjects(emptyList(), emptyList(), all).map { it.id })
        assertEquals(
            listOf("a", "old"),
            orderClientProjects(emptyList(), emptyList(), all, keep = "old").map { it.id },
        )
    }

    @Test
    fun `effective id prefers the span override over the project`() {
        val byId = mapOf("p" to project("p", "ap"))
        assertEquals("ap", effectiveClientProjectId(span("1", "p"), byId))
        assertEquals("x", effectiveClientProjectId(span("2", "p", "x"), byId))
        assertEquals(null, effectiveClientProjectId(span("3"), byId))
    }

    @Test
    fun `client project edit is keep set or inherit`() {
        assertTrue(clientProjectEdit("a", "a") is ClientProjectEdit.Keep)
        assertTrue(clientProjectEdit(null, null) is ClientProjectEdit.Keep)
        assertTrue(clientProjectEdit("a", null) is ClientProjectEdit.Inherit)
        val set = clientProjectEdit(null, "b")
        assertEquals("b", (set as ClientProjectEdit.Set).clientProjectId)
    }
}
