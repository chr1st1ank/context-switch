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
import dev.contextswitch.LogbookStore

@Composable
fun ManageScreen(store: LogbookStore) {
    val snap by store.snapshot.collectAsState()
    var tab by remember { mutableIntStateOf(0) }
    var showAdd by remember { mutableStateOf(false) }

    Column(Modifier.fillMaxSize()) {
        TabRow(selectedTabIndex = tab) {
            Tab(selected = tab == 0, onClick = { tab = 0 }, text = { Text("📁 Projects") })
            Tab(selected = tab == 1, onClick = { tab = 1 }, text = { Text("🏷️ Tags") })
        }
        LazyColumn(Modifier.weight(1f)) {
            if (tab == 0) {
                items(snap?.projects.orEmpty(), key = { it.id }) { p ->
                    ManageRow(
                        name = p.name,
                        color = projectColor(p.id),
                        client = p.client,
                        archived = p.archived,
                        onRename = { store.renameProject(p.id, it) },
                        onSetClient = { store.setProjectClient(p.id, it) },
                        onArchive = { store.setProjectArchived(p.id, !p.archived) },
                    )
                }
            } else {
                items(snap?.tags.orEmpty(), key = { it.id }) { t ->
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
            ExtendedFloatingActionButton(onClick = { showAdd = true }, icon = { Icon(Icons.Filled.Add, null) }, text = { Text(if (tab == 0) "New project" else "New tag") })
        }
    }

    if (showAdd) {
        var name by remember { mutableStateOf("") }
        var client by remember { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { showAdd = false },
            title = { Text(if (tab == 0) "New project" else "New tag") },
            text = {
                Column {
                    OutlinedTextField(
                        value = name,
                        onValueChange = { name = it },
                        singleLine = true,
                        label = { Text("Name") },
                    )
                    if (tab == 0) {
                        OutlinedTextField(
                            value = client,
                            onValueChange = { client = it },
                            singleLine = true,
                            label = { Text("Client (optional)") },
                        )
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    if (name.isNotBlank()) {
                        if (tab == 0) {
                            store.addProject(name.trim(), client.trim().ifBlank { null })
                        } else {
                            store.addTag(name.trim())
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
    archived: Boolean,
    onRename: (String) -> Unit,
    onSetClient: ((String?) -> Unit)? = null,
    onArchive: () -> Unit,
) {
    var editing by remember { mutableStateOf(false) }
    ListItem(
        leadingContent = color?.let { c ->
            { Box(Modifier.size(12.dp).background(c, CircleShape)) }
        },
        headlineContent = { Text(if (archived) "$name (archived)" else name) },
        supportingContent = client?.let { c -> { Text(c) } },
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
        AlertDialog(
            onDismissRequest = { editing = false },
            title = { Text(if (onSetClient != null) "Edit project" else "Rename") },
            text = {
                Column {
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
                    }
                    editing = false
                }) { Text("Save") }
            },
            dismissButton = { TextButton(onClick = { editing = false }) { Text("Cancel") } },
        )
    }
}
