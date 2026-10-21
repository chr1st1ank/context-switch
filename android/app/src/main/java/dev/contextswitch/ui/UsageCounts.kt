package dev.contextswitch.ui

import dev.contextswitch.ProjectRec
import dev.contextswitch.SpanRec
import dev.contextswitch.TagRec

/** A project + tag-set combination observed in span history. */
internal data class RecentCombo(
    val projectId: String?,
    val projectName: String,
    val tagIds: Set<String>,
    val tagNames: List<String>,
    val count: Int,
    /** Index of the newest span containing this combo (lower = more recent). */
    val firstSeen: Int,
)

internal data class UsageResult(
    /** All non-archived projects, most-used first. */
    val projects: List<ProjectRec>,
    /** All non-archived tags, most-used first. */
    val tags: List<TagRec>,
    /** Recent (project, tag-set) combinations, most-used first. May slightly exceed the quota. */
    val combos: List<RecentCombo>,
)

private data class ComboKey(val projectId: String?, val tagIds: Set<String>)

private class Counter<K> {
    val counts = LinkedHashMap<K, Int>()
    private val firstSeen = HashMap<K, Int>()

    fun add(key: K, index: Int) {
        counts.merge(key, 1, Int::plus)
        firstSeen.putIfAbsent(key, index)
    }

    fun count(key: K) = counts[key] ?: 0
    fun firstSeenAt(key: K) = firstSeen[key] ?: Int.MAX_VALUE
}

/**
 * Usage statistics over span history, driving the timer screen's section ordering and
 * the Recent section.
 *
 * Spans are scanned newest-first until [quota] distinct projects AND [quota] distinct
 * tags AND [quota] distinct (project, tag-set) combos have been seen — or the history
 * is exhausted. Everything seen inside that window counts; later spans are ignored.
 */
internal fun computeUsage(
    spans: List<SpanRec>,
    projects: List<ProjectRec>,
    tags: List<TagRec>,
    quota: Int = 10,
): UsageResult {
    val projectCounter = Counter<String>()
    val tagCounter = Counter<String>()
    val comboCounter = Counter<ComboKey>()

    for ((i, span) in spans.withIndex()) {
        span.projectId?.let { projectCounter.add(it, i) }
        span.tagIds.forEach { tagCounter.add(it, i) }
        if (span.projectId != null || span.tagIds.isNotEmpty()) {
            comboCounter.add(ComboKey(span.projectId, span.tagIds.toSet()), i)
        }
        if (projectCounter.counts.size >= quota &&
            tagCounter.counts.size >= quota &&
            comboCounter.counts.size >= quota
        ) break
    }

    fun <T> usageOrder(id: (T) -> String, name: (T) -> String, counter: Counter<String>) =
        compareByDescending<T> { counter.count(id(it)) }
            .thenBy { counter.firstSeenAt(id(it)) }
            .thenBy(String.CASE_INSENSITIVE_ORDER, name)
            .thenBy(selector = id)

    val orderedProjects = projects.filter { !it.archived }
        .sortedWith(usageOrder({ it.id }, { it.name }, projectCounter))
    val orderedTags = tags.filter { !it.archived }
        .sortedWith(usageOrder({ it.id }, { it.name }, tagCounter))

    val projectById = projects.associateBy { it.id }
    val tagById = tags.associateBy { it.id }

    val combos = comboCounter.counts.mapNotNull { (key, count) ->
        val project = key.projectId?.let { projectById[it]?.takeUnless { p -> p.archived } }
        if (key.projectId != null && project == null) return@mapNotNull null
        val resolvedTags = key.tagIds.mapNotNull { tagById[it]?.takeUnless { t -> t.archived } }
        if (resolvedTags.size != key.tagIds.size) return@mapNotNull null
        RecentCombo(
            projectId = key.projectId,
            projectName = project?.name ?: "unassigned",
            tagIds = key.tagIds,
            tagNames = resolvedTags.map { it.name }.sortedWith(String.CASE_INSENSITIVE_ORDER),
            count = count,
            firstSeen = comboCounter.firstSeenAt(key),
        )
    }.sortedWith(
        compareByDescending<RecentCombo> { it.count }
            .thenBy { it.firstSeen }
            .thenBy(String.CASE_INSENSITIVE_ORDER) { it.projectName + it.tagNames.joinToString("") }
            .thenBy { it.projectId ?: "" },
    )

    return UsageResult(orderedProjects, orderedTags, combos)
}
