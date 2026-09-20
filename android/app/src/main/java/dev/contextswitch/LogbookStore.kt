package dev.contextswitch

import android.content.Context
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Owns the UniFFI [CoswStore] and the latest committed [SnapshotRec].
 * All provider calls run on [Dispatchers.IO]; the last snapshot is cached to
 * disk so the UI can render offline (read-only, per the architecture).
 */
class LogbookStore(private val context: Context) {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val cacheFile get() = context.filesDir.resolve("logbook_cache.json")
    private val locationFile get() = context.filesDir.resolve("location.txt")

    private var store: CoswStore? = null

    val snapshot = MutableStateFlow<SnapshotRec?>(null)
    val error = MutableStateFlow<String?>(null)
    val configured = MutableStateFlow(false)
    val busy = MutableStateFlow(false)

    val hasActiveTimer: Boolean get() = snapshot.value?.activeSpanId != null

    /** The last committed snapshot, falling back to the on-disk cache. */
    fun cachedSnapshot(): SnapshotRec? {
        snapshot.value?.let { return it }
        loadCache()
        return snapshot.value
    }

    fun open() {
        val settings = SettingsStore(context)
        if (!settings.isConfigured()) {
            configured.value = false
            loadCache()
            return
        }
        configured.value = true
        scope.launch {
            try {
                store = CoswStore.open(settings.storageConfig())
                refresh()
            } catch (e: MobileException) {
                error.value = e.message
                loadCache()
            }
        }
    }

    fun refresh() = runOp { snapshot() }

    fun start(projectId: String?, tagIds: List<String>) = runOp { start(projectId, tagIds) }
    fun stop() = runOp { stop() }
    fun switchTo(projectId: String?, tagIds: List<String>) = runOp { `switch`(projectId, tagIds) }
    fun cancel() = runOp { cancel() }
    fun addSpan(start: String, stop: String, projectId: String?, tagIds: List<String>) =
        runOp { addSpan(start, stop, projectId, tagIds) }
    fun editSpan(spanId: String, start: String?, stop: String?, tagIds: List<String>?) =
        runOp { editSpan(spanId, start, stop, tagIds) }
    fun assignProject(spanId: String, projectId: String?) = runOp { assignProject(spanId, projectId) }
    fun removeSpan(spanId: String) = runOp { removeSpan(spanId) }
    fun addProject(name: String) = runOp { addProject(name) }
    fun renameProject(id: String, name: String) = runOp { renameProject(id, name) }
    fun setProjectArchived(id: String, archived: Boolean) = runOp { setProjectArchived(id, archived) }
    fun addTag(name: String) = runOp { addTag(name) }
    fun renameTag(id: String, name: String) = runOp { renameTag(id, name) }
    fun setTagArchived(id: String, archived: Boolean) = runOp { setTagArchived(id, archived) }

    fun testConnection(onResult: (String) -> Unit) {
        scope.launch {
            val result = try {
                val s = CoswStore.open(SettingsStore(context).storageConfig())
                "OK: ${s.testConnection()}"
            } catch (e: MobileException) {
                "Error: ${e.message}"
            }
            withContext(Dispatchers.Main) { onResult(result) }
        }
    }

    private fun runOp(op: suspend CoswStore.() -> SnapshotRec) {
        val s = store ?: return
        scope.launch {
            busy.value = true
            try {
                val snap = s.op()
                snapshot.value = snap
                error.value = null
                cacheFile.writeText(snap.logbookJson)
                locationFile.writeText(snap.locationUrl)
            } catch (e: MobileException) {
                error.value = e.message
            } finally {
                busy.value = false
            }
        }
    }

    private fun loadCache() {
        val json = runCatching { cacheFile.readText() }.getOrNull() ?: return
        val location = runCatching { locationFile.readText() }.getOrDefault("")
        runCatching { snapshot.value = snapshotFromJson(json, location) }
    }
}
