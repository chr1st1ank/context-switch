package dev.contextswitch.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import dev.contextswitch.LogbookStore
import dev.contextswitch.PortableConfigRec
import dev.contextswitch.SettingsStore
import dev.contextswitch.parsePortableConfig
import dev.contextswitch.serializePortableConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
fun SettingsScreen(store: LogbookStore) {
    val context = LocalContext.current
    val settings = remember { SettingsStore(context) }
    var testResult by remember { mutableStateOf<String?>(null) }
    val location = store.snapshot.collectAsState().value?.locationUrl

    var bucket by remember { mutableStateOf(settings.bucket) }
    var region by remember { mutableStateOf(settings.region) }
    var prefix by remember { mutableStateOf(settings.prefix) }
    var endpoint by remember { mutableStateOf(settings.endpoint) }
    var pathStyle by remember { mutableStateOf(settings.usePathStyle) }
    var accessKey by remember { mutableStateOf(settings.accessKeyId) }
    var secretKey by remember { mutableStateOf(settings.secretAccessKey) }
    var sessionToken by remember { mutableStateOf(settings.sessionToken) }
    var passphrase by remember { mutableStateOf(settings.passphrase) }

    val scope = rememberCoroutineScope()
    var importCandidate by remember { mutableStateOf<PortableConfigRec?>(null) }
    var fileStatus by remember { mutableStateOf<String?>(null) }

    val importLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch(Dispatchers.IO) {
            val result = runCatching {
                val text = context.contentResolver.openInputStream(uri)
                    ?.use { it.readBytes().decodeToString() }
                    ?: error("could not open selected file")
                parsePortableConfig(text)
            }
            withContext(Dispatchers.Main) {
                result.onSuccess { importCandidate = it }
                    .onFailure { fileStatus = "Import failed: ${it.message}" }
            }
        }
    }

    val exportLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream"),
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        val text = serializePortableConfig(
            settings.bucket,
            settings.region,
            settings.prefix,
            settings.endpoint.ifBlank { null },
            settings.usePathStyle,
        )
        scope.launch(Dispatchers.IO) {
            val result = runCatching {
                context.contentResolver.openOutputStream(uri)
                    ?.use { it.write(text.toByteArray()) }
                    ?: error("could not open selected file")
            }
            withContext(Dispatchers.Main) {
                fileStatus = result.fold(
                    onSuccess = { "Config exported" },
                    onFailure = { "Export failed: ${it.message}" },
                )
            }
        }
    }

    fun save() {
        settings.bucket = bucket.trim()
        settings.region = region.trim()
        settings.prefix = prefix.trim()
        settings.endpoint = endpoint.trim()
        settings.usePathStyle = pathStyle
        settings.accessKeyId = accessKey.trim()
        settings.secretAccessKey = secretKey.trim()
        settings.sessionToken = sessionToken.trim()
        settings.passphrase = passphrase
        store.open()
    }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("S3 backend", style = MaterialTheme.typography.titleMedium)
        Field("Bucket", bucket) { bucket = it }
        Field("Region", region) { region = it }
        Field("Prefix", prefix) { prefix = it }
        Field("Endpoint (optional)", endpoint) { endpoint = it }
        Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
            Checkbox(checked = pathStyle, onCheckedChange = { pathStyle = it })
            Text("Path-style URLs (MinIO etc.)")
        }
        Field("Access key ID", accessKey) { accessKey = it }
        SecretField("Secret access key", secretKey) { secretKey = it }
        Field("Session token (optional)", sessionToken) { sessionToken = it }
        SecretField("Encryption passphrase", passphrase) { passphrase = it }

        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Button(onClick = { save() }) { Text("Save & connect") }
            OutlinedButton(onClick = {
                save()
                testResult = null
                store.testConnection { testResult = it }
            }) { Text("Test") }
        }
        testResult?.let { Text(it) }

        HorizontalDivider()
        Text("Config file", style = MaterialTheme.typography.titleMedium)
        Text(
            "Import or export a cosw config.toml. Credentials and the " +
                "encryption passphrase are never included.",
            style = MaterialTheme.typography.bodySmall,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedButton(onClick = {
                fileStatus = null
                importLauncher.launch(
                    arrayOf("application/toml", "text/*", "application/octet-stream"),
                )
            }) { Text("Import…") }
            OutlinedButton(onClick = {
                fileStatus = null
                exportLauncher.launch("context-switch.toml")
            }) { Text("Export…") }
        }
        fileStatus?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
        location?.let { Text("Storage: $it", style = MaterialTheme.typography.bodySmall) }
    }

    importCandidate?.let { rec ->
        AlertDialog(
            onDismissRequest = { importCandidate = null },
            title = { Text("Import config") },
            text = {
                Text(
                    buildString {
                        append(
                            "Replace the current storage settings (bucket, region, " +
                                "prefix, endpoint, path style) with this file's values? " +
                                "Credentials and passphrase are unchanged.",
                        )
                        if (rec.warnings.isNotEmpty()) {
                            append("\n\nSkipped keys:\n")
                            append(rec.warnings.joinToString("\n"))
                        }
                    },
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    settings.importPortableConfig(rec)
                    bucket = rec.bucket
                    region = rec.region
                    prefix = rec.prefix
                    endpoint = rec.endpoint.orEmpty()
                    pathStyle = rec.usePathStyle
                    importCandidate = null
                    store.open()
                }) { Text("Replace") }
            },
            dismissButton = {
                TextButton(onClick = { importCandidate = null }) { Text("Cancel") }
            },
        )
    }
}

@Composable
private fun Field(label: String, value: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii),
    )
}

@Composable
private fun SecretField(label: String, value: String, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
        visualTransformation = PasswordVisualTransformation(),
    )
}
