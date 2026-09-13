package dev.contextswitch

import android.content.Context
import androidx.core.content.edit

/**
 * Storage backend configuration. Non-secret fields live in private
 * SharedPreferences; AWS keys and the envelope passphrase go to a
 * Keystore-backed ("secret") preferences file so they are encrypted at rest.
 */
class SettingsStore(context: Context) {

    private val prefs = context.getSharedPreferences("config", Context.MODE_PRIVATE)
    private val secrets = context.getSharedPreferences("secrets", Context.MODE_PRIVATE)

    var provider: String
        get() = prefs.getString("provider", "s3") ?: "s3"
        set(v) = prefs.edit { putString("provider", v) }

    var bucket: String
        get() = prefs.getString("bucket", "") ?: ""
        set(v) = prefs.edit { putString("bucket", v) }

    var region: String
        get() = prefs.getString("region", "") ?: ""
        set(v) = prefs.edit { putString("region", v) }

    var prefix: String
        get() = prefs.getString("prefix", "") ?: ""
        set(v) = prefs.edit { putString("prefix", v) }

    var endpoint: String
        get() = prefs.getString("endpoint", "") ?: ""
        set(v) = prefs.edit { putString("endpoint", v) }

    var usePathStyle: Boolean
        get() = prefs.getBoolean("use_path_style", false)
        set(v) = prefs.edit { putBoolean("use_path_style", v) }

    var accessKeyId: String
        get() = secrets.getString("access_key_id", "") ?: ""
        set(v) = secrets.edit { putString("access_key_id", v) }

    var secretAccessKey: String
        get() = secrets.getString("secret_access_key", "") ?: ""
        set(v) = secrets.edit { putString("secret_access_key", v) }

    var sessionToken: String
        get() = secrets.getString("session_token", "") ?: ""
        set(v) = secrets.edit { putString("session_token", v) }

    var passphrase: String
        get() = secrets.getString("passphrase", "") ?: ""
        set(v) = secrets.edit { putString("passphrase", v) }

    fun isConfigured(): Boolean =
        bucket.isNotBlank() && region.isNotBlank() &&
            accessKeyId.isNotBlank() && secretAccessKey.isNotBlank() &&
            passphrase.isNotBlank()

    fun storageConfig(): StorageConfig =
        StorageConfig.S3(
            S3Config(
                bucket = bucket,
                region = region,
                prefix = prefix,
                endpoint = endpoint.ifBlank { null },
                usePathStyle = usePathStyle,
                accessKeyId = accessKeyId,
                secretAccessKey = secretAccessKey,
                sessionToken = sessionToken.ifBlank { null },
                passphrase = passphrase,
            ),
        )
}
