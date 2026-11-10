package dev.contextswitch.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import dev.contextswitch.ClientProjectRec
import dev.contextswitch.LogbookStore

private const val TAB_PROJECTS = 0
private const val TAB_CLIENT_PROJECTS = 1
private const val TAB_TAGS = 2

@Composable
fun ManageScreen(store: LogbookStore) {
    val snap by store.snapshot.collectAsState()
    var tab by remember { mutableIntStateOf(TAB_PROJECTS) }
    var showAdd by remember { mutableStateOf(false) }
    val clientProjects = snap?.clientProjects.orEmpty()
    val clientProjectName = clientProjects.associate { it.id to it.name }

    Column(Modifier.fillMaxSize()) {
        TabRow(selectedTabIndex = tab) {
            Tab(selected = tab == TAB_PROJECTS, onClick = { tab = TAB_PROJECTS }, text = { Text("📁 Projects") })
            Tab(selected = tab == TAB_CLIENT_PROJECTS, onClick = { tab = TAB_CLIENT_PROJECTS }, text = { Text("🧾 Client projects") })
            Tab(selected = tab == TAB_TAGS, onClick = { tab = TAB_TAGS }, text = { Text("🏷️ Tags") })
        }
        LazyColumn(Modifier.weight(1f)) {
            when (tab) {
                TAB_PROJECTS -> items(snap?.projects.orEmpty(), key = { it.id }) { p ->
                    ManageRow(
                        name = p.name,
                        color = projectColor(p.id),
                        client = p.client,
                        clientProjectId = p.clientProjectId,
                        clientProjectName = p.clientProjectId?.let { clientProjectName[it] },
                        clientProjects = clientProjects,
                        archived = p.archived,
                        onRename = { store.renameProject(p.id, it) },
                        onSetClient = { store.setProjectClient(p.id, it) },
                        onSetClientProject = { store.setProjectClientProject(p.id, it) },
                        onArchive = { store.setProjectArchived(p.id, !p.archived) },
                    )
                }
                TAB_CLIENT_PROJECTS -> items(clientProjects, key = { it.id }) { c ->
                    ManageRow(
                        name = c.name,
                        archived = c.archived,
                        onRename = { store.renameClientProject(c.id, it) },
                        onArchive = { store.setClientProjectArchived(c.id, !c.archived) },
                    )
                }
                else -> items(snap?.tags.orEmpty(), key = { it.id }) { t ->
                    ManageRow(
                        name = t.name,
                        archived = t.archived,
                        onRename = { store.renameTag(t.id, it) },
                        onArchive = { store.setTagArchived(t.id, !t.archived) },
                    )
                }
            }
        }
        Row(Modifier.padding(16.dp)) {
            val label = when (tab) {
                TAB_PROJECTS -> "New project"
                TAB_CLIENT_PROJECTS -> "New client project"
                else -> "New tag"
            }
            ExtendedFloatingActionButton(onClick = { showAdd = true }, icon = { Icon(Icons.Filled.Add, null) }, text = { Text(label) })
        }
    }

    if (showAdd) {
        var name by remember { mutableStateOf("") }
        var client by remember { mutableStateOf("") }
        var clientProjectId by remember { mutableStateOf<String?>(null) }
        AlertDialog(
            onDismissRequest = { showAdd = false },
            title = {
                Text(
                    when (tab) {
                        TAB_PROJECTS -> "New project"
                        TAB_CLIENT_PROJECTS -> "New client project"
                        else -> "New tag"
                    },
                )
            },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(
                        value = name,
                        onValueChange = { name = it },
                        singleLine = true,
                        label = { Text("Name") },
                    )
                    if (tab == TAB_PROJECTS) {
                        OutlinedTextField(
                            value = client,
                            onValueChange = { client = it },
                            singleLine = true,
                            label = { Text("Client (optional)") },
                        )
                        if (clientProjects.any { !it.archived }) {
                            Text("Client project", style = MaterialTheme.typography.labelMedium)
                            ClientProjectChips(
                                selectedId = clientProjectId,
                                options = clientProjects.filter { !it.archived }
                                    .sortedBy { it.name.lowercase() },
                                noneLabel = "None",
                                onSelect = { clientProjectId = it },
                            )
                        }
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    if (name.isNotBlank()) {
                        when (tab) {
                            TAB_PROJECTS -> store.addProject(
                                name.trim(),
                                client.trim().ifBlank { null },
                                clientProjectId,
                            )
                            TAB_CLIENT_PROJECTS -> store.addClientProject(name.trim())
                            else -> store.addTag(name.trim())
                        }
                    }
                    showAdd = false
                }) { Text("Add") }
            },
            dismissButton = { TextButton(onClick = { showAdd = false }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun ManageRow(
    name: String,
    color: Color? = null,
    client: String? = null,
    clientProjectId: String? = null,
    clientProjectName: String? = null,
    clientProjects: List<ClientProjectRec> = emptyList(),
    archived: Boolean,
    onRename: (String) -> Unit,
    onSetClient: ((String?) -> Unit)? = null,
    onSetClientProject: ((String?) -> Unit)? = null,
    onArchive: () -> Unit,
) {
    var editing by remember { mutableStateOf(false) }
    ListItem(
        leadingContent = color?.let { c ->
            { Box(Modifier.size(12.dp).background(c, CircleShape)) }
        },
        headlineContent = { Text(if (archived) "$name (archived)" else name) },
        supportingContent = listOfNotNull(client, clientProjectName).joinToString(" · ")
            .takeIf { it.isNotEmpty() }
            ?.let { s -> { Text(s) } },
        trailingContent = {
            Row {
                TextButton(onClick = { editing = true }) { Text(if (onSetClient != null) "Edit" else "Rename") }
                TextButton(onClick = onArchive) { Text(if (archived) "Unarchive" else "Archive") }
            }
        },
    )
    HorizontalDivider()
    if (editing) {
        var newName by remember { mutableStateOf(name) }
        var newClient by remember { mutableStateOf(client.orEmpty()) }
        var newClientProjectId by remember { mutableStateOf(clientProjectId) }
        AlertDialog(
            onDismissRequest = { editing = false },
            title = { Text(if (onSetClient != null) "Edit project" else "Rename") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(
                        value = newName,
                        onValueChange = { newName = it },
                        singleLine = true,
                        label = { Text("Name") },
                    )
                    if (onSetClient != null) {
                        OutlinedTextField(
                            value = newClient,
                            onValueChange = { newClient = it },
                            singleLine = true,
                            label = { Text("Client") },
                        )
                    }
                    if (onSetClientProject != null &&
                        (clientProjects.any { !it.archived } || clientProjectId != null)
                    ) {
                        Text("Client project", style = MaterialTheme.typography.labelMedium)
                        ClientProjectChips(
                            selectedId = newClientProjectId,
                            options = clientProjects
                                .filter { !it.archived || it.id == newClientProjectId }
                                .sortedBy { it.name.lowercase() },
                            noneLabel = "None",
                            onSelect = { newClientProjectId = it },
                        )
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    if (newName.isNotBlank()) {
                        if (newName.trim() != name) onRename(newName.trim())
                        onSetClient?.let { setClient ->
                            val c = newClient.trim().ifBlank { null }
                            if (c != client) setClient(c)
                        }
                        onSetClientProject?.let { setClientProject ->
                            if (newClientProjectId != clientProjectId) setClientProject(newClientProjectId)
                        }
                    }
                    editing = false
                }) { Text("Save") }
            },
            dismissButton = { TextButton(onClick = { editing = false }) { Text("Cancel") } },
        )
    }
}
