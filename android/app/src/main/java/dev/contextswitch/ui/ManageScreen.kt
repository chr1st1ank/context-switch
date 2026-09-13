package dev.contextswitch.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.contextswitch.LogbookStore

@Composable
fun ManageScreen(store: LogbookStore) {
    val snap by store.snapshot.collectAsState()
    var tab by remember { mutableStateOf(0) }
    var showAdd by remember { mutableStateOf(false) }

    Column(Modifier.fillMaxSize()) {
        TabRow(selectedTabIndex = tab) {
            Tab(selected = tab == 0, onClick = { tab = 0 }, text = { Text("Projects") })
            Tab(selected = tab == 1, onClick = { tab = 1 }, text = { Text("Tags") })
        }
        LazyColumn(Modifier.weight(1f)) {
            if (tab == 0) {
                items(snap?.projects.orEmpty(), key = { it.id }) { p ->
                    ManageRow(
                        name = p.name,
                        archived = p.archived,
                        onRename = { store.renameProject(p.id, it) },
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
        AlertDialog(
            onDismissRequest = { showAdd = false },
            title = { Text(if (tab == 0) "New project" else "New tag") },
            text = { OutlinedTextField(value = name, onValueChange = { name = it }, singleLine = true) },
            confirmButton = {
                TextButton(onClick = {
                    if (name.isNotBlank()) {
                        if (tab == 0) store.addProject(name.trim()) else store.addTag(name.trim())
                    }
                    showAdd = false
                }) { Text("Add") }
            },
            dismissButton = { TextButton(onClick = { showAdd = false }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun ManageRow(name: String, archived: Boolean, onRename: (String) -> Unit, onArchive: () -> Unit) {
    var renaming by remember { mutableStateOf(false) }
    ListItem(
        headlineContent = { Text(if (archived) "$name (archived)" else name) },
        trailingContent = {
            Row {
                TextButton(onClick = { renaming = true }) { Text("Rename") }
                TextButton(onClick = onArchive) { Text(if (archived) "Unarchive" else "Archive") }
            }
        },
    )
    HorizontalDivider()
    if (renaming) {
        var newName by remember { mutableStateOf(name) }
        AlertDialog(
            onDismissRequest = { renaming = false },
            title = { Text("Rename") },
            text = { OutlinedTextField(value = newName, onValueChange = { newName = it }, singleLine = true) },
            confirmButton = {
                TextButton(onClick = { if (newName.isNotBlank()) onRename(newName.trim()); renaming = false }) { Text("Save") }
            },
            dismissButton = { TextButton(onClick = { renaming = false }) { Text("Cancel") } },
        )
    }
}
