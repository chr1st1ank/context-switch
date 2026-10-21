package dev.contextswitch.ui

import dev.contextswitch.ProjectRec
import dev.contextswitch.SpanRec
import dev.contextswitch.TagRec
import org.junit.Assert.assertEquals
import org.junit.Test

class UsageCountsTest {
    private fun span(
        id: String,
        projectId: String? = null,
        tagIds: List<String> = emptyList(),
        isActive: Boolean = false,
    ) = SpanRec(id, "2025-06-15T10:00:00Z", null, projectId, tagIds, null, isActive)

    private fun project(id: String, name: String = id, archived: Boolean = false) =
        ProjectRec(id, name, null, archived)

    private fun tag(id: String, name: String = id, archived: Boolean = false) =
        TagRec(id, name, archived)

    @Test
    fun `empty history yields entities in name order and no combos`() {
        val result = computeUsage(
            spans = emptyList(),
            projects = listOf(project("b", "Beta"), project("a", "alpha")),
            tags = listOf(tag("y"), tag("x")),
        )
        assertEquals(listOf("a", "b"), result.projects.map { it.id })
        assertEquals(listOf("x", "y"), result.tags.map { it.id })
        assertEquals(emptyList<RecentCombo>(), result.combos)
    }

    @Test
    fun `orders entities by count then recency then name`() {
        // newest first: p2(1) > p3(1, older) > p1(0) alphabetical last
        val spans = listOf(
            span("s1", projectId = "p2", tagIds = listOf("t1")),
            span("s2", projectId = "p3", tagIds = listOf("t1")),
        )
        val result = computeUsage(
            spans,
            projects = listOf(project("p1", "aaa"), project("p2"), project("p3")),
            tags = listOf(tag("t1"), tag("t2")),
        )
        assertEquals(listOf("p2", "p3", "p1"), result.projects.map { it.id })
        assertEquals(listOf("t1", "t2"), result.tags.map { it.id })
    }

    @Test
    fun `count wins over recency`() {
        val spans = listOf(
            span("s1", projectId = "p2"),
            span("s2", projectId = "p1"),
            span("s3", projectId = "p1"),
        )
        val result = computeUsage(spans, listOf(project("p1"), project("p2")), emptyList())
        assertEquals(listOf("p1", "p2"), result.projects.map { it.id })
    }

    @Test
    fun `combos are exact tuples sorted by count`() {
        val spans = listOf(
            span("s1", projectId = "p1", tagIds = listOf("t1", "t2")),
            span("s2", projectId = "p1", tagIds = listOf("t1")),
            span("s3", projectId = "p1", tagIds = listOf("t1", "t2")),
        )
        val result = computeUsage(spans, listOf(project("p1", "Acme")),
            listOf(tag("t1", "one"), tag("t2", "two")))
        assertEquals(2, result.combos.size)
        assertEquals(listOf("t1", "t2"), result.combos[0].tagIds.toList().sorted())
        assertEquals(2, result.combos[0].count)
        assertEquals(listOf("one", "two"), result.combos[0].tagNames)
        assertEquals(1, result.combos[1].count)
    }

    @Test
    fun `tag order within a combo does not create distinct tuples`() {
        val spans = listOf(
            span("s1", projectId = "p1", tagIds = listOf("t1", "t2")),
            span("s2", projectId = "p1", tagIds = listOf("t2", "t1")),
        )
        val result = computeUsage(spans, listOf(project("p1")), listOf(tag("t1"), tag("t2")))
        assertEquals(1, result.combos.size)
        assertEquals(2, result.combos[0].count)
    }

    @Test
    fun `fully empty span produces no combo but still counts nothing`() {
        val result = computeUsage(listOf(span("s1")), emptyList(), emptyList())
        assertEquals(emptyList<RecentCombo>(), result.combos)
    }

    @Test
    fun `projectless tagged span becomes an unassigned combo`() {
        val result = computeUsage(
            listOf(span("s1", tagIds = listOf("t1"))),
            emptyList(),
            listOf(tag("t1", "home")),
        )
        assertEquals(1, result.combos.size)
        assertEquals(null, result.combos[0].projectId)
        assertEquals("unassigned", result.combos[0].projectName)
    }

    @Test
    fun `combos referencing archived or missing entities are dropped`() {
        val spans = listOf(
            span("s1", projectId = "old", tagIds = listOf("t1")),
            span("s2", projectId = "p1", tagIds = listOf("gone")),
            span("s3", projectId = "p1", tagIds = listOf("t1")),
        )
        val result = computeUsage(
            spans,
            projects = listOf(project("old", archived = true), project("p1")),
            tags = listOf(tag("t1"), tag("gone", archived = true)),
        )
        assertEquals(1, result.combos.size)
        assertEquals("p1", result.combos[0].projectId)
        assertEquals(setOf("t1"), result.combos[0].tagIds)
    }

    @Test
    fun `archived entities are excluded from ordered lists`() {
        val spans = listOf(span("s1", projectId = "p1", tagIds = listOf("t1")))
        val result = computeUsage(
            spans,
            listOf(project("p1"), project("p2", archived = true)),
            listOf(tag("t1"), tag("t2", archived = true)),
        )
        assertEquals(listOf("p1"), result.projects.map { it.id })
        assertEquals(listOf("t1"), result.tags.map { it.id })
    }

    @Test
    fun `scan stops once all quotas are filled`() {
        // quota=1: after s1 we have 1 project, 1 tag, 1 combo -> stop.
        // p2/t2 in s2 must not be counted.
        val spans = listOf(
            span("s1", projectId = "p1", tagIds = listOf("t1")),
            span("s2", projectId = "p2", tagIds = listOf("t2")),
        )
        val result = computeUsage(
            spans,
            listOf(project("p1"), project("p2")),
            listOf(tag("t1"), tag("t2")),
            quota = 1,
        )
        // p2/t2 unseen -> count 0 -> sorted after p1/t1
        assertEquals(listOf("p1", "p2"), result.projects.map { it.id })
        assertEquals(listOf("t1", "t2"), result.tags.map { it.id })
        assertEquals(1, result.combos.size)
    }

    @Test
    fun `scan continues until every quota is met`() {
        // quota=2: s1 fills projects? no — only 1 distinct project so far.
        // Scanning must continue into s2 even though combos are already full.
        val spans = listOf(
            span("s1", projectId = "p1", tagIds = listOf("t1")),
            span("s2", projectId = "p1", tagIds = listOf("t1")),
            span("s3", projectId = "p2", tagIds = listOf("t2")),
        )
        val result = computeUsage(
            spans,
            listOf(project("p1"), project("p2")),
            listOf(tag("t1"), tag("t2")),
            quota = 2,
        )
        assertEquals(listOf("p1", "p2"), result.projects.map { it.id })
        // s1 and s2 form the same combo; s3 adds a second distinct combo.
        assertEquals(2, result.combos.size)
        assertEquals(2, result.combos[0].count)
    }
}
