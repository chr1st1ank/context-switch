//! Portable `config.toml` support: parse and serialize the cosw client
//! config format so non-secret storage settings can move between devices.
//!
//! The cosw TOML file is the interchange format — there is no separate
//! portable schema. Only the `[storage]` table's non-secret fields are
//! understood; per ADR-0006 secrets (AWS credentials, the encryption
//! passphrase) never live in the file, so an imported config can never
//! fully provision a device on its own.
//!
//! Parsing mirrors the cosw loader (`cli/cosw/config.py`): unknown keys
//! warn-and-ignore for forward compatibility, known-but-unusable keys
//! (`data_file`, `passphrase_command`, `profile`) warn so the importing
//! client can surface them, and only `provider = "s3"` is accepted —
//! remote-capable clients are S3-only.

use thiserror::Error;
use toml::Table;

/// Errors produced while reading a portable config file.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PortableConfigError {
    /// The file is not valid TOML.
    #[error("invalid config file: {0}")]
    Parse(String),
    /// The config targets a provider the importing client cannot use.
    #[error("unsupported storage provider {0:?} (must be 's3')")]
    UnsupportedProvider(String),
    /// A key exists but has the wrong type or an unusable value.
    #[error("{0}")]
    Invalid(String),
}

/// Non-secret storage settings parsed from a cosw `config.toml`, plus
/// human-readable warnings for every key that was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableConfig {
    pub bucket: String,
    pub region: String,
    pub prefix: String,
    pub endpoint: Option<String>,
    pub use_path_style: bool,
    /// One entry per ignored key: unknown keys (forward compatibility)
    /// and keys that are meaningful to cosw but not to this client.
    pub warnings: Vec<String>,
}

const KNOWN_TOP_LEVEL: &[&str] = &["storage"];
const KNOWN_STORAGE_KEYS: &[&str] = &[
    "provider",
    "data_file",
    "bucket",
    "region",
    "prefix",
    "endpoint",
    "use_path_style",
    "profile",
    "passphrase_command",
];
/// Valid cosw keys that carry no meaning for an S3-only client.
const IGNORED_STORAGE_KEYS: &[&str] = &["data_file", "passphrase_command", "profile"];

/// Parse a cosw `config.toml` into validated storage settings.
///
/// Returns [`PortableConfigError::Parse`] for malformed TOML,
/// [`PortableConfigError::UnsupportedProvider`] when `provider` is not
/// `"s3"` (absent defaults to `"local"`, matching cosw), and
/// [`PortableConfigError::Invalid`] for wrong-typed or empty values.
/// Every skipped key is reported in [`PortableConfig::warnings`].
pub fn parse(text: &str) -> Result<PortableConfig, PortableConfigError> {
    let raw: Table = text
        .parse()
        .map_err(|e: toml::de::Error| PortableConfigError::Parse(e.to_string()))?;
    let mut warnings = Vec::new();

    for key in raw.keys() {
        if !KNOWN_TOP_LEVEL.contains(&key.as_str()) {
            warnings.push(format!("ignoring unknown key {key:?}"));
        }
    }

    let empty_storage = Table::new();
    let storage = match raw.get("storage") {
        None => &empty_storage,
        Some(toml::Value::Table(t)) => t,
        Some(_) => {
            return Err(PortableConfigError::Invalid(
                "'storage' must be a table".to_string(),
            ))
        }
    };
    for key in storage.keys() {
        if IGNORED_STORAGE_KEYS.contains(&key.as_str()) {
            warnings.push(format!(
                "ignoring 'storage.{key}' (not supported on this device)"
            ));
        } else if !KNOWN_STORAGE_KEYS.contains(&key.as_str()) {
            warnings.push(format!("ignoring unknown key 'storage.{key}'"));
        }
    }

    let provider = match storage.get("provider") {
        None => "local",
        Some(toml::Value::String(s)) => s.as_str(),
        Some(_) => {
            return Err(PortableConfigError::Invalid(
                "'storage.provider' must be a string".to_string(),
            ))
        }
    };
    if provider != "s3" {
        return Err(PortableConfigError::UnsupportedProvider(
            provider.to_string(),
        ));
    }

    let bucket = required_string(storage, "bucket")?;
    let region = required_string(storage, "region")?;
    let prefix = optional_string(storage, "prefix")?.unwrap_or_default();
    let endpoint = optional_string(storage, "endpoint")?;
    let use_path_style = match storage.get("use_path_style") {
        None => false,
        Some(toml::Value::Boolean(b)) => b.to_owned(),
        Some(_) => {
            return Err(PortableConfigError::Invalid(
                "'storage.use_path_style' must be a boolean".to_string(),
            ))
        }
    };

    Ok(PortableConfig {
        bucket,
        region,
        prefix,
        endpoint,
        use_path_style,
        warnings,
    })
}

fn required_string(storage: &Table, key: &str) -> Result<String, PortableConfigError> {
    match storage.get(key) {
        Some(toml::Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        _ => Err(PortableConfigError::Invalid(format!(
            "'storage.{key}' is required and must be a non-empty string for 's3' provider"
        ))),
    }
}

fn optional_string(storage: &Table, key: &str) -> Result<Option<String>, PortableConfigError> {
    match storage.get(key) {
        None => Ok(None),
        Some(toml::Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
        Some(_) => Err(PortableConfigError::Invalid(format!(
            "'storage.{key}' must be a non-empty string"
        ))),
    }
}

/// Serialize storage settings as a cosw-compatible `config.toml`.
///
/// The output is canonical: `provider`, `bucket`, and `region` always,
/// optional keys only when they differ from cosw's defaults. Comments and
/// key order from an imported file are not preserved.
pub fn serialize(
    bucket: &str,
    region: &str,
    prefix: &str,
    endpoint: Option<&str>,
    use_path_style: bool,
) -> String {
    let mut out = String::from(
        "# cosw configuration — context-switch\n\
         # Credentials and the encryption passphrase are never included.\n\
         \n\
         [storage]\n\
         provider = \"s3\"\n",
    );
    out.push_str(&format!("bucket = {}\n", quoted(bucket)));
    out.push_str(&format!("region = {}\n", quoted(region)));
    if !prefix.is_empty() {
        out.push_str(&format!("prefix = {}\n", quoted(prefix)));
    }
    if let Some(endpoint) = endpoint {
        out.push_str(&format!("endpoint = {}\n", quoted(endpoint)));
    }
    if use_path_style {
        out.push_str("use_path_style = true\n");
    }
    out
}

/// Quote a string as a TOML basic string, escaping as needed.
fn quoted(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
[storage]
provider = "s3"
bucket = "my-bucket"
region = "eu-central-1"
prefix = "logs/"
endpoint = "https://s3.example.com"
use_path_style = true
profile = "work"
passphrase_command = "pass show context-switch"
"#;

    #[test]
    fn parses_full_s3_config() {
        let cfg = parse(FULL).unwrap();
        assert_eq!(cfg.bucket, "my-bucket");
        assert_eq!(cfg.region, "eu-central-1");
        assert_eq!(cfg.prefix, "logs/");
        assert_eq!(cfg.endpoint.as_deref(), Some("https://s3.example.com"));
        assert!(cfg.use_path_style);
        assert_eq!(cfg.warnings.len(), 2);
        assert!(cfg.warnings.iter().any(|w| w.contains("profile")));
        assert!(cfg
            .warnings
            .iter()
            .any(|w| w.contains("passphrase_command")));
    }

    #[test]
    fn minimal_config_gets_defaults() {
        let cfg = parse("[storage]\nprovider = \"s3\"\nbucket = \"b\"\nregion = \"r\"\n").unwrap();
        assert_eq!(cfg.prefix, "");
        assert_eq!(cfg.endpoint, None);
        assert!(!cfg.use_path_style);
        assert!(cfg.warnings.is_empty());
    }

    #[test]
    fn local_provider_is_rejected() {
        let err = parse("[storage]\nprovider = \"local\"\n").unwrap_err();
        assert_eq!(
            err,
            PortableConfigError::UnsupportedProvider("local".to_string())
        );
    }

    #[test]
    fn absent_provider_defaults_to_local_and_is_rejected() {
        let err = parse("[storage]\nbucket = \"b\"\nregion = \"r\"\n").unwrap_err();
        assert_eq!(
            err,
            PortableConfigError::UnsupportedProvider("local".to_string())
        );
    }

    #[test]
    fn missing_bucket_and_region_are_errors() {
        for text in [
            "[storage]\nprovider = \"s3\"\nregion = \"r\"\n",
            "[storage]\nprovider = \"s3\"\nbucket = \"b\"\n",
            "[storage]\nprovider = \"s3\"\nbucket = \"\"\nregion = \"r\"\n",
        ] {
            assert!(matches!(parse(text), Err(PortableConfigError::Invalid(_))));
        }
    }

    #[test]
    fn unknown_keys_warn_instead_of_failing() {
        let cfg = parse(
            "[future]\nx = 1\n\n[storage]\nprovider = \"s3\"\nbucket = \"b\"\nregion = \"r\"\nnew_key = 1\n",
        )
        .unwrap();
        assert_eq!(cfg.warnings.len(), 2);
        assert!(cfg.warnings.iter().any(|w| w.contains("future")));
        assert!(cfg.warnings.iter().any(|w| w.contains("storage.new_key")));
    }

    #[test]
    fn malformed_toml_is_a_parse_error() {
        let err = parse("[storage\nprovider = ").unwrap_err();
        assert!(matches!(err, PortableConfigError::Parse(_)));
    }

    #[test]
    fn wrong_types_are_invalid() {
        for text in [
            "storage = 1\n",
            "[storage]\nprovider = 3\n",
            "[storage]\nprovider = \"s3\"\nbucket = \"b\"\nregion = \"r\"\nuse_path_style = \"yes\"\n",
            "[storage]\nprovider = \"s3\"\nbucket = \"b\"\nregion = \"r\"\nendpoint = \"\"\n",
        ] {
            assert!(
                matches!(parse(text), Err(PortableConfigError::Invalid(_))),
                "expected Invalid for {text:?}"
            );
        }
    }

    #[test]
    fn serialize_emits_canonical_minimal_toml() {
        let text = serialize("b", "r", "", None, false);
        assert_eq!(
            text,
            "# cosw configuration — context-switch\n\
             # Credentials and the encryption passphrase are never included.\n\
             \n\
             [storage]\n\
             provider = \"s3\"\n\
             bucket = \"b\"\n\
             region = \"r\"\n"
        );
    }

    #[test]
    fn serialize_includes_non_default_optionals() {
        let text = serialize("b", "r", "logs/", Some("https://s3.example.com"), true);
        assert!(text.contains("prefix = \"logs/\""));
        assert!(text.contains("endpoint = \"https://s3.example.com\""));
        assert!(text.contains("use_path_style = true"));
    }

    #[test]
    fn serialize_quotes_strings_safely() {
        // Whatever quoting style the serializer picks (basic or literal),
        // the result must parse back to the exact original values.
        let text = serialize("buck\"et", "r", "it's\nmultiline", None, false);
        let cfg = parse(&text).unwrap();
        assert_eq!(cfg.bucket, "buck\"et");
        assert_eq!(cfg.prefix, "it's\nmultiline");
    }

    #[test]
    fn round_trips_through_parse() {
        let text = serialize("b", "r", "p/", Some("https://e.example"), true);
        let cfg = parse(&text).unwrap();
        assert_eq!(cfg.bucket, "b");
        assert_eq!(cfg.region, "r");
        assert_eq!(cfg.prefix, "p/");
        assert_eq!(cfg.endpoint.as_deref(), Some("https://e.example"));
        assert!(cfg.use_path_style);
        assert!(cfg.warnings.is_empty());
    }
}
