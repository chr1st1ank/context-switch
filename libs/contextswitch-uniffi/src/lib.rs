//! UniFFI bindings exposing contextswitch-core to the Android client.
//!
//! The surface is deliberately flat: a [`CoswStore`] object wraps a storage
//! provider, every mutation performs read → domain op → conditional commit
//! (with one automatic retry on [`MobileError::Conflict`]), and returns a
//! fresh [`SnapshotRec`] so the UI always renders committed state.
//!
//! All timestamps cross the FFI as RFC 3339 / ISO 8601 strings; all
//! identifiers as UUID strings. S3 credentials are injected explicitly via
//! [`S3Config`] — env vars and `~/.aws` do not exist on Android.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use contextswitch_core::blob::BlobStore;
use contextswitch_core::cache::{
    CommitMode, MemoryCachingProvider, SyncStatus, DEFAULT_REFRESH_INTERVAL,
};
use contextswitch_core::crypto::EnvelopeCipher;
use contextswitch_core::domain::{DomainError, Logbook};
use contextswitch_core::provider::GenericProvider;
use contextswitch_core::s3::{AwsCredentials, S3BlobStore};
use contextswitch_core::storage::{LocalFsProvider, StorageError, StorageProvider};
use uuid::Uuid;

uniffi::setup_scaffolding!();

/// How the store reaches canonical data.
#[derive(uniffi::Enum)]
pub enum StorageConfig {
    /// Local filesystem logbook (dev/testing).
    Local { path: String },
    /// S3-compatible object storage with mandatory envelope encryption.
    S3(S3Config),
}

/// S3 backend parameters plus explicitly injected credentials.
#[derive(uniffi::Record)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub prefix: String,
    pub endpoint: Option<String>,
    pub use_path_style: bool,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    /// Envelope passphrase; required — remote storage is always encrypted.
    pub passphrase: String,
}

#[derive(uniffi::Record)]
pub struct ProjectRec {
    pub id: String,
    pub name: String,
    pub archived: bool,
}

#[derive(uniffi::Record)]
pub struct TagRec {
    pub id: String,
    pub name: String,
    pub archived: bool,
}

#[derive(uniffi::Record)]
pub struct SpanRec {
    pub id: String,
    /// RFC 3339 UTC timestamp.
    pub started_at: String,
    pub stopped_at: Option<String>,
    pub project_id: Option<String>,
    pub tag_ids: Vec<String>,
    pub is_active: bool,
}

/// A point-in-time view of the canonical logbook, safe to render.
#[derive(uniffi::Record)]
pub struct SnapshotRec {
    /// Opaque version to commit against; the logbook revision.
    pub version: String,
    /// Where the provider reads/writes (`s3://…` or `file://…`).
    pub location_url: String,
    pub active_span_id: Option<String>,
    /// All spans, newest first.
    pub spans: Vec<SpanRec>,
    pub projects: Vec<ProjectRec>,
    pub tags: Vec<TagRec>,
    /// Serialized logbook; clients may cache it for offline display.
    pub logbook_json: String,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("write conflict: {0}")]
    Conflict(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("storage unavailable: {0}")]
    Unavailable(String),
    #[error("corrupt logbook: {0}")]
    Corrupt(String),
    #[error("decryption failed (wrong passphrase or tampered data)")]
    DecryptionFailed,
    #[error("storage is locked by another process")]
    Locked,
    #[error("invalid data: {0}")]
    Invalid(String),
    #[error("I/O error: {0}")]
    Io(String),
}

impl From<StorageError> for MobileError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::Io(e) => MobileError::Io(e.to_string()),
            StorageError::Corrupt(m) => MobileError::Corrupt(m),
            StorageError::Conflict { expected, actual } => {
                MobileError::Conflict(format!("expected {expected}, current {actual}"))
            }
            StorageError::Locked => MobileError::Locked,
            StorageError::InvalidData(e) => MobileError::Invalid(e.to_string()),
            StorageError::DecryptionFailed => MobileError::DecryptionFailed,
            StorageError::Unauthorized(m) => MobileError::Unauthorized(m),
            StorageError::Unavailable(m) => MobileError::Unavailable(m),
        }
    }
}

impl From<DomainError> for MobileError {
    fn from(e: DomainError) -> Self {
        MobileError::Invalid(e.to_string())
    }
}

/// Aggregate state of the write buffer and background revalidation.
/// Always `Clean` when the store was opened without a cache.
#[derive(uniffi::Enum)]
pub enum StoreSyncStatus {
    /// No pending writes and no refresh failure.
    Clean,
    /// This many buffered commits are still queued or being written.
    Pending(u32),
    /// The last background refresh failed; the cached snapshot is still
    /// served. Carries the error's display text.
    Degraded(String),
}

/// A buffered commit the storage backend rejected; the mutation is
/// preserved verbatim so the app can export or retry it.
#[derive(uniffi::Record)]
pub struct FailedWriteRec {
    /// The logbook as it was submitted, serialized as JSON.
    pub logbook_json: String,
    /// Why the write was rejected (error display text).
    pub error: String,
}

/// A storage provider plus the read-mutate-commit loop the app drives.
#[derive(uniffi::Object)]
pub struct CoswStore {
    provider: Arc<dyn StorageProvider>,
    /// Set when opened via [`CoswStore::open_cached`]; kept to expose the
    /// write-buffer status surface.
    cache: Option<Arc<MemoryCachingProvider>>,
    location_url: String,
}

fn parse_time(s: &str) -> Result<DateTime<Utc>, MobileError> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| MobileError::Invalid(format!("bad timestamp {s:?}: {e}")))
}

fn parse_uuid(s: &str) -> Result<Uuid, MobileError> {
    Uuid::parse_str(s).map_err(|e| MobileError::Invalid(format!("bad uuid {s:?}: {e}")))
}

fn parse_uuid_opt(s: &Option<String>) -> Result<Option<Uuid>, MobileError> {
    s.as_deref().map(parse_uuid).transpose()
}

fn parse_uuid_vec(v: &[String]) -> Result<Vec<Uuid>, MobileError> {
    v.iter().map(|s| parse_uuid(s)).collect()
}

fn snapshot_rec(
    snap: contextswitch_core::storage::StorageSnapshot,
    location_url: &str,
) -> Result<SnapshotRec, MobileError> {
    let lb = &snap.logbook;
    let logbook_json =
        serde_json::to_string(lb).map_err(|e| MobileError::Corrupt(e.to_string()))?;
    let mut spans: Vec<SpanRec> = lb
        .spans
        .values()
        .map(|s| SpanRec {
            id: s.id.to_string(),
            started_at: s.started_at.to_rfc3339(),
            stopped_at: s.stopped_at.map(|t| t.to_rfc3339()),
            project_id: s.project_id.map(|id| id.to_string()),
            tag_ids: s.tag_ids.iter().map(Uuid::to_string).collect(),
            is_active: s.is_active(),
        })
        .collect();
    spans.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(SnapshotRec {
        version: snap.version,
        location_url: location_url.to_string(),
        active_span_id: lb.active_span_id.map(|id| id.to_string()),
        spans,
        projects: lb
            .projects
            .values()
            .map(|p| ProjectRec {
                id: p.id.to_string(),
                name: p.name.clone(),
                archived: p.archived,
            })
            .collect(),
        tags: lb
            .tags
            .values()
            .map(|t| TagRec {
                id: t.id.to_string(),
                name: t.name.clone(),
                archived: t.archived,
            })
            .collect(),
        logbook_json,
    })
}

fn open_provider(config: StorageConfig) -> Result<(Arc<dyn StorageProvider>, String), MobileError> {
    match config {
        StorageConfig::Local { path } => {
            let provider = LocalFsProvider::new(&path)?;
            let location_url = format!("file://{path}");
            Ok((Arc::new(provider), location_url))
        }
        StorageConfig::S3(cfg) => {
            let creds = AwsCredentials {
                access_key_id: cfg.access_key_id,
                secret_access_key: cfg.secret_access_key,
                session_token: cfg.session_token,
            };
            let blob_store: Arc<dyn BlobStore> = Arc::new(S3BlobStore::with_credentials(
                cfg.bucket.clone(),
                cfg.region,
                cfg.prefix.clone(),
                cfg.endpoint,
                cfg.use_path_style,
                creds,
            ));
            let provider = GenericProvider::new(
                blob_store,
                Arc::new(EnvelopeCipher::new()),
                Some(cfg.passphrase),
            );
            let key = if cfg.prefix.is_empty() {
                "logbook.json".to_string()
            } else if cfg.prefix.ends_with('/') {
                format!("{}logbook.json", cfg.prefix)
            } else {
                format!("{}/logbook.json", cfg.prefix)
            };
            Ok((Arc::new(provider), format!("s3://{}/{}", cfg.bucket, key)))
        }
    }
}

#[uniffi::export]
impl CoswStore {
    /// Open a store for the given backend. Local paths are created on first
    /// commit; S3 performs no network I/O until the first read/write.
    #[uniffi::constructor]
    pub fn open(config: StorageConfig) -> Result<Arc<Self>, MobileError> {
        let (provider, location_url) = open_provider(config)?;
        Ok(Arc::new(Self {
            provider,
            cache: None,
            location_url,
        }))
    }

    /// Open a store behind an in-memory caching provider: `read()` and
    /// commits never block on storage — commits are optimistic and a
    /// background worker writes them through in order while revalidating
    /// the snapshot every `refresh_interval_secs` seconds (0 = default).
    /// Watch [`CoswStore::sync_status`]/[`CoswStore::take_failed_write`]
    /// and call [`CoswStore::flush`] on app pause/exit.
    #[uniffi::constructor]
    pub fn open_cached(
        config: StorageConfig,
        refresh_interval_secs: u64,
    ) -> Result<Arc<Self>, MobileError> {
        let (inner, location_url) = open_provider(config)?;
        let interval = if refresh_interval_secs == 0 {
            DEFAULT_REFRESH_INTERVAL
        } else {
            Duration::from_secs(refresh_interval_secs)
        };
        let cache = Arc::new(MemoryCachingProvider::new(
            inner,
            interval,
            CommitMode::Buffered,
        ));
        Ok(Arc::new(Self {
            provider: cache.clone(),
            cache: Some(cache),
            location_url,
        }))
    }

    /// Buffered commits queued or in flight; always 0 without a cache.
    pub fn pending_writes(&self) -> u32 {
        self.cache
            .as_ref()
            .map(|c| c.pending_writes() as u32)
            .unwrap_or(0)
    }

    /// Buffer/revalidation state for status displays.
    pub fn sync_status(&self) -> StoreSyncStatus {
        match self.cache.as_ref().map(|c| c.sync_status()) {
            Some(SyncStatus::Pending(n)) => StoreSyncStatus::Pending(n as u32),
            Some(SyncStatus::Degraded(e)) => StoreSyncStatus::Degraded(e),
            _ => StoreSyncStatus::Clean,
        }
    }

    /// The oldest buffered write the backend rejected, if any. Consumes
    /// it — call repeatedly to drain the failed-writes list.
    pub fn take_failed_write(&self) -> Result<Option<FailedWriteRec>, MobileError> {
        let Some(failed) = self.cache.as_ref().and_then(|c| c.take_failed_write()) else {
            return Ok(None);
        };
        let logbook_json = serde_json::to_string(&failed.logbook)
            .map_err(|e| MobileError::Corrupt(e.to_string()))?;
        Ok(Some(FailedWriteRec {
            logbook_json,
            error: failed.error.to_string(),
        }))
    }

    /// Wait until all commits queued so far have been written (or
    /// rejected); returns the earliest write failure. Call on app
    /// pause/exit. No-op without a cache.
    pub fn flush(&self) -> Result<(), MobileError> {
        match &self.cache {
            Some(cache) => cache.flush().map_err(MobileError::from),
            None => Ok(()),
        }
    }

    /// Read the current canonical state.
    pub fn snapshot(&self) -> Result<SnapshotRec, MobileError> {
        snapshot_rec(self.provider.read()?, &self.location_url)
    }

    /// Connectivity/credentials check: performs a read and returns the
    /// resolved location URL.
    pub fn test_connection(&self) -> Result<String, MobileError> {
        let _ = self.provider.read()?;
        Ok(self.location_url.clone())
    }

    /// Start a timer for `project_id` (None = unassigned) with `tag_ids`.
    pub fn start(
        &self,
        project_id: Option<String>,
        tag_ids: Vec<String>,
    ) -> Result<SnapshotRec, MobileError> {
        let project_id = parse_uuid_opt(&project_id)?;
        let tag_ids = parse_uuid_vec(&tag_ids)?;
        self.mutate(|lb| {
            lb.start_timer(Utc::now(), project_id, tag_ids.clone())
                .map(|_| ())
        })
    }

    /// Stop the active timer.
    pub fn stop(&self) -> Result<SnapshotRec, MobileError> {
        self.mutate(|lb| lb.stop_timer(Utc::now()).map(|_| ()))
    }

    /// Stop the active timer (if any) and start a new span in one commit.
    pub fn switch(
        &self,
        project_id: Option<String>,
        tag_ids: Vec<String>,
    ) -> Result<SnapshotRec, MobileError> {
        let project_id = parse_uuid_opt(&project_id)?;
        let tag_ids = parse_uuid_vec(&tag_ids)?;
        self.mutate(|lb| {
            lb.switch(Utc::now(), project_id, tag_ids.clone())
                .map(|_| ())
        })
    }

    /// Discard the active timer without recording the time.
    pub fn cancel(&self) -> Result<SnapshotRec, MobileError> {
        self.mutate(|lb| {
            if let Some(id) = lb.active_span_id {
                lb.remove_span(id)?;
            }
            Ok(())
        })
    }

    /// Record a span that was not tracked live.
    pub fn add_span(
        &self,
        started_at: String,
        stopped_at: String,
        project_id: Option<String>,
        tag_ids: Vec<String>,
    ) -> Result<SnapshotRec, MobileError> {
        let started = parse_time(&started_at)?;
        let stopped = parse_time(&stopped_at)?;
        let project_id = parse_uuid_opt(&project_id)?;
        let tag_ids = parse_uuid_vec(&tag_ids)?;
        self.mutate(|lb| {
            lb.add_span(started, stopped, project_id, tag_ids.clone(), Utc::now())
                .map(|_| ())
        })
    }

    /// Edit a stopped span's times and tags. `None` leaves a field unchanged.
    /// `project_id` is managed via [`CoswStore::assign_project`].
    pub fn edit_span(
        &self,
        span_id: String,
        started_at: Option<String>,
        stopped_at: Option<String>,
        tag_ids: Option<Vec<String>>,
    ) -> Result<SnapshotRec, MobileError> {
        let span_id = parse_uuid(&span_id)?;
        let started = started_at.map(|s| parse_time(&s)).transpose()?;
        let stopped = stopped_at.map(|s| parse_time(&s)).transpose()?;
        let tag_ids = tag_ids.map(|v| parse_uuid_vec(&v)).transpose()?;
        self.mutate(|lb| lb.edit_span(span_id, Utc::now(), started, stopped, None, tag_ids.clone()))
    }

    /// Set or clear (`None`) a span's project assignment.
    pub fn assign_project(
        &self,
        span_id: String,
        project_id: Option<String>,
    ) -> Result<SnapshotRec, MobileError> {
        let span_id = parse_uuid(&span_id)?;
        let project_id = parse_uuid_opt(&project_id)?;
        self.mutate(|lb| match project_id {
            Some(p) => lb.edit_span(span_id, Utc::now(), None, None, Some(p), None),
            None => lb.unassign_span(span_id, Utc::now()),
        })
    }

    /// Remove a span entirely.
    pub fn remove_span(&self, span_id: String) -> Result<SnapshotRec, MobileError> {
        let span_id = parse_uuid(&span_id)?;
        self.mutate(|lb| lb.remove_span(span_id))
    }

    pub fn add_project(&self, name: String) -> Result<SnapshotRec, MobileError> {
        self.mutate(|lb| lb.add_project(name.clone(), Utc::now()).map(|_| ()))
    }

    pub fn rename_project(
        &self,
        project_id: String,
        name: String,
    ) -> Result<SnapshotRec, MobileError> {
        let project_id = parse_uuid(&project_id)?;
        self.mutate(|lb| lb.rename_project(project_id, name.clone(), Utc::now()))
    }

    pub fn set_project_archived(
        &self,
        project_id: String,
        archived: bool,
    ) -> Result<SnapshotRec, MobileError> {
        let project_id = parse_uuid(&project_id)?;
        self.mutate(|lb| lb.set_project_archived(project_id, archived, Utc::now()))
    }

    pub fn add_tag(&self, name: String) -> Result<SnapshotRec, MobileError> {
        self.mutate(|lb| lb.add_tag(name.clone(), Utc::now()).map(|_| ()))
    }

    pub fn rename_tag(&self, tag_id: String, name: String) -> Result<SnapshotRec, MobileError> {
        let tag_id = parse_uuid(&tag_id)?;
        self.mutate(|lb| lb.rename_tag(tag_id, name.clone(), Utc::now()))
    }

    pub fn set_tag_archived(
        &self,
        tag_id: String,
        archived: bool,
    ) -> Result<SnapshotRec, MobileError> {
        let tag_id = parse_uuid(&tag_id)?;
        self.mutate(|lb| lb.set_tag_archived(tag_id, archived, Utc::now()))
    }
}

impl CoswStore {
    /// Read → apply `f` → conditional commit; one automatic retry on
    /// [`StorageError::Conflict`], then the conflict is surfaced. The
    /// returned snapshot is built from the logbook that was just committed —
    /// a re-read would fetch exactly those bytes at the cost of a round
    /// trip.
    fn mutate(
        &self,
        f: impl Fn(&mut Logbook) -> Result<(), DomainError>,
    ) -> Result<SnapshotRec, MobileError> {
        for attempt in 0..2 {
            let snap = self.provider.read()?;
            let mut lb = snap.logbook.clone();
            f(&mut lb)?;
            match self.provider.commit(lb.clone(), &snap.version) {
                Ok(version) => {
                    lb.revision = version.parse().unwrap_or(lb.revision);
                    return snapshot_rec(
                        contextswitch_core::storage::StorageSnapshot {
                            version,
                            logbook: lb,
                        },
                        &self.location_url,
                    );
                }
                Err(StorageError::Conflict { .. }) if attempt == 0 => continue,
                Err(e) => return Err(e.into()),
            }
        }
        unreachable!()
    }
}

/// Rebuild a display snapshot from a cached `logbook_json` (offline reads).
#[uniffi::export]
pub fn snapshot_from_json(
    logbook_json: String,
    location_url: String,
) -> Result<SnapshotRec, MobileError> {
    let lb: Logbook =
        serde_json::from_str(&logbook_json).map_err(|e| MobileError::Corrupt(e.to_string()))?;
    snapshot_rec(
        contextswitch_core::storage::StorageSnapshot {
            version: lb.revision.to_string(),
            logbook: lb,
        },
        &location_url,
    )
}
