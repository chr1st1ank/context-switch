//! Domain model for context-switch.
//!
//! Core types: [`Span`], [`Project`], [`Tag`], and the canonical [`Logbook`]
//! that aggregates them. Every mutation takes an explicit `at` timestamp so
//! timer actions captured while offline can be replayed faithfully against
//! canonical data. The Python bindings expose the same types and rules.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::exceptions;

/// The only schema version this library reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

/// Errors produced by domain mutations and logbook validation.
#[derive(Debug, Error)]
pub enum DomainError {
    /// A timer is already active; the contained ID is the active span.
    #[error("a timer is already active (span {0})")]
    AlreadyActive(Uuid),
    /// No timer is active.
    #[error("no active timer")]
    NoActiveTimer,
    /// Referenced project does not exist.
    #[error("unknown project: {0}")]
    ProjectNotFound(Uuid),
    /// Referenced tag does not exist.
    #[error("unknown tag: {0}")]
    TagNotFound(Uuid),
    /// Referenced span does not exist.
    #[error("unknown span: {0}")]
    SpanNotFound(Uuid),
    /// A project with this name already exists (case-insensitive).
    #[error("duplicate project name: {0:?}")]
    DuplicateProjectName(String),
    /// A tag with this name already exists (case-insensitive).
    #[error("duplicate tag name: {0:?}")]
    DuplicateTagName(String),
    /// `started_at` must not be later than `stopped_at`.
    #[error("started_at must not be later than stopped_at")]
    InvalidTimeRange,
    /// The span overlaps an existing span; the contained ID is the span
    /// it conflicts with. Time must not be double-recorded.
    #[error("span overlaps existing span {0}")]
    Overlap(Uuid),
    /// `stopped_at` cannot be set or cleared by editing a span.
    #[error("stopped_at cannot be set or cleared by editing a span")]
    InvalidStoppedAtEdit,
    /// `active_span_id` does not match the unique unstopped span.
    #[error("active_span_id does not match the unique unstopped span")]
    InconsistentActiveSpan,
    /// The logbook's schema version is not supported by this library.
    #[error("unsupported schema version: {0}")]
    UnsupportedSchemaVersion(u32),
}

impl From<DomainError> for PyErr {
    fn from(e: DomainError) -> PyErr {
        exceptions::DomainError::new_err(e.to_string())
    }
}

fn parse_uuid(value: &str) -> PyResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|e| PyValueError::new_err(format!("invalid UUID {value:?}: {e}")))
}

fn parse_uuid_opt(value: Option<String>) -> PyResult<Option<Uuid>> {
    value.as_deref().map(parse_uuid).transpose()
}

fn parse_uuid_vec(values: Option<Vec<String>>) -> PyResult<Vec<Uuid>> {
    values
        .unwrap_or_default()
        .iter()
        .map(|s| parse_uuid(s))
        .collect()
}

/// A named work context to which time can be assigned.
#[pyclass]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Stable identity; never reused.
    pub id: Uuid,
    /// Display name; case-insensitively unique within a logbook.
    #[pyo3(get)]
    pub name: String,
    /// Archived projects are hidden from pickers but keep their history.
    #[pyo3(get)]
    pub archived: bool,
    #[pyo3(get)]
    pub created_at: DateTime<Utc>,
    #[pyo3(get)]
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn new(name: impl Into<String>, at: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            archived: false,
            created_at: at,
            updated_at: at,
        }
    }
}

#[pymethods]
impl Project {
    #[new]
    fn py_new(name: String) -> Self {
        Self::new(name, Utc::now())
    }

    #[getter]
    fn id(&self) -> String {
        self.id.to_string()
    }

    fn __repr__(&self) -> String {
        format!(
            "Project(id={}, name={:?}, archived={})",
            self.id, self.name, self.archived
        )
    }
}

/// A reusable label that can be attached to a span.
#[pyclass]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    /// Stable identity; never reused.
    pub id: Uuid,
    /// Display name; case-insensitively unique within a logbook.
    #[pyo3(get)]
    pub name: String,
    /// Archived tags are hidden from pickers but keep their history.
    #[pyo3(get)]
    pub archived: bool,
    #[pyo3(get)]
    pub created_at: DateTime<Utc>,
    #[pyo3(get)]
    pub updated_at: DateTime<Utc>,
}

impl Tag {
    pub fn new(name: impl Into<String>, at: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            archived: false,
            created_at: at,
            updated_at: at,
        }
    }
}

#[pymethods]
impl Tag {
    #[new]
    fn py_new(name: String) -> Self {
        Self::new(name, Utc::now())
    }

    #[getter]
    fn id(&self) -> String {
        self.id.to_string()
    }

    fn __repr__(&self) -> String {
        format!(
            "Tag(id={}, name={:?}, archived={})",
            self.id, self.name, self.archived
        )
    }
}

/// A mutable record of a period during which the user records time.
#[pyclass]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    /// Stable identity; never reused.
    pub id: Uuid,
    #[pyo3(get)]
    pub started_at: DateTime<Utc>,
    #[pyo3(get)]
    pub stopped_at: Option<DateTime<Utc>>,
    /// Assigned project; `None` is unassigned time awaiting classification.
    pub project_id: Option<Uuid>,
    pub tag_ids: Vec<Uuid>,
    #[pyo3(get)]
    pub created_at: DateTime<Utc>,
    #[pyo3(get)]
    pub updated_at: DateTime<Utc>,
}

impl Span {
    pub fn new(at: DateTime<Utc>, project_id: Option<Uuid>, tag_ids: Vec<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            started_at: at,
            stopped_at: None,
            project_id,
            tag_ids,
            created_at: at,
            updated_at: at,
        }
    }

    /// Whether this span is the active timer (its stop time was never recorded).
    pub fn is_active(&self) -> bool {
        self.stopped_at.is_none()
    }
}

/// Whether two `[start, end)` intervals intersect. A `None` end means the
/// interval is still running and extends indefinitely, so the active span
/// occupies everything from its start onward.
fn intervals_overlap(
    a_start: DateTime<Utc>,
    a_end: Option<DateTime<Utc>>,
    b_start: DateTime<Utc>,
    b_end: Option<DateTime<Utc>>,
) -> bool {
    a_end.is_none_or(|end| b_start < end) && b_end.is_none_or(|end| a_start < end)
}

#[pymethods]
impl Span {
    #[new]
    #[pyo3(signature = (started_at, project_id=None, tag_ids=None))]
    fn py_new(
        started_at: DateTime<Utc>,
        project_id: Option<String>,
        tag_ids: Option<Vec<String>>,
    ) -> PyResult<Self> {
        Ok(Self::new(
            started_at,
            parse_uuid_opt(project_id)?,
            parse_uuid_vec(tag_ids)?,
        ))
    }

    #[getter]
    fn id(&self) -> String {
        self.id.to_string()
    }

    #[getter]
    fn project_id(&self) -> Option<String> {
        self.project_id.map(|id| id.to_string())
    }

    #[getter]
    fn tag_ids(&self) -> Vec<String> {
        self.tag_ids.iter().map(Uuid::to_string).collect()
    }

    #[getter(is_active)]
    fn py_is_active(&self) -> bool {
        self.is_active()
    }

    fn __repr__(&self) -> String {
        format!(
            "Span(id={}, started_at={}, stopped_at={:?})",
            self.id, self.started_at, self.stopped_at
        )
    }
}

/// The canonical versioned data logbook: projects, tags, spans, and the
/// identity of the active span.
///
/// `active_span_id` is the authoritative pointer to the one active span and
/// must always agree with `stopped_at`: it is either `None` with no unstopped
/// span, or it points at the unique span whose `stopped_at` is `None`. It only
/// changes as part of a switch (old span stopped, new one started at the same
/// instant) or a stop (cleared to `None`). A logbook where it disagrees with
/// the spans is corrupt; [`Logbook::validate`] detects that.
#[pyclass]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Logbook {
    #[pyo3(get)]
    pub schema_version: u32,
    /// Monotonic commit counter; owned by the storage provider.
    #[pyo3(get)]
    pub revision: u64,
    pub active_span_id: Option<Uuid>,
    pub projects: BTreeMap<Uuid, Project>,
    pub tags: BTreeMap<Uuid, Tag>,
    pub spans: BTreeMap<Uuid, Span>,
}

impl Default for Logbook {
    fn default() -> Self {
        Self::new()
    }
}

impl Logbook {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            revision: 0,
            active_span_id: None,
            projects: BTreeMap::new(),
            tags: BTreeMap::new(),
            spans: BTreeMap::new(),
        }
    }

    /// The span currently recording time, if any.
    pub fn active_span(&self) -> Option<&Span> {
        self.active_span_id.and_then(|id| self.spans.get(&id))
    }

    /// Check every domain invariant. Providers run this on read and before
    /// commit, so a logbook that passes is safe to treat as canonical.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(DomainError::UnsupportedSchemaVersion(self.schema_version));
        }
        let mut unstopped = Vec::new();
        for span in self.spans.values() {
            if let Some(project_id) = span.project_id {
                if !self.projects.contains_key(&project_id) {
                    return Err(DomainError::ProjectNotFound(project_id));
                }
            }
            for tag_id in &span.tag_ids {
                if !self.tags.contains_key(tag_id) {
                    return Err(DomainError::TagNotFound(*tag_id));
                }
            }
            match span.stopped_at {
                Some(stopped) if span.started_at > stopped => {
                    return Err(DomainError::InvalidTimeRange);
                }
                None => unstopped.push(span.id),
                _ => {}
            }
        }
        if let Some(name) = find_duplicate(self.projects.values().map(|p| &p.name)) {
            return Err(DomainError::DuplicateProjectName(name));
        }
        if let Some(name) = find_duplicate(self.tags.values().map(|t| &t.name)) {
            return Err(DomainError::DuplicateTagName(name));
        }
        match self.active_span_id {
            Some(id) if unstopped == [id] => {}
            None if unstopped.is_empty() => {}
            _ => return Err(DomainError::InconsistentActiveSpan),
        }
        // No two spans may overlap. Sorted by start, an overlap always
        // shows up between adjacent spans; the unstopped span runs
        // unbounded and so conflicts with anything that starts after it.
        let mut ordered: Vec<&Span> = self.spans.values().collect();
        ordered.sort_by_key(|span| span.started_at);
        for pair in ordered.windows(2) {
            let (prev, next) = (pair[0], pair[1]);
            if prev.stopped_at.is_none_or(|end| next.started_at < end) {
                return Err(DomainError::Overlap(prev.id));
            }
        }
        Ok(())
    }

    fn check_project_ref(&self, project_id: Option<Uuid>) -> Result<(), DomainError> {
        match project_id {
            Some(id) if !self.projects.contains_key(&id) => Err(DomainError::ProjectNotFound(id)),
            _ => Ok(()),
        }
    }

    /// Verify every referenced tag exists; sort and deduplicate the list.
    fn normalize_tags(&self, mut tag_ids: Vec<Uuid>) -> Result<Vec<Uuid>, DomainError> {
        for id in &tag_ids {
            if !self.tags.contains_key(id) {
                return Err(DomainError::TagNotFound(*id));
            }
        }
        tag_ids.sort();
        tag_ids.dedup();
        Ok(tag_ids)
    }

    fn check_project_name(&self, name: &str, except: Option<Uuid>) -> Result<(), DomainError> {
        if self
            .projects
            .values()
            .any(|p| Some(p.id) != except && p.name.to_lowercase() == name.to_lowercase())
        {
            return Err(DomainError::DuplicateProjectName(name.to_string()));
        }
        Ok(())
    }

    fn check_tag_name(&self, name: &str, except: Option<Uuid>) -> Result<(), DomainError> {
        if self
            .tags
            .values()
            .any(|t| Some(t.id) != except && t.name.to_lowercase() == name.to_lowercase())
        {
            return Err(DomainError::DuplicateTagName(name.to_string()));
        }
        Ok(())
    }

    /// Reject a `[started_at, stopped_at)` interval that overlaps an
    /// existing span. `stopped_at` of `None` is the active timer and
    /// extends indefinitely. `except` skips the span being replaced.
    fn check_overlap(
        &self,
        started_at: DateTime<Utc>,
        stopped_at: Option<DateTime<Utc>>,
        except: Option<Uuid>,
    ) -> Result<(), DomainError> {
        for span in self.spans.values() {
            if Some(span.id) == except {
                continue;
            }
            if intervals_overlap(started_at, stopped_at, span.started_at, span.stopped_at) {
                return Err(DomainError::Overlap(span.id));
            }
        }
        Ok(())
    }

    /// Start a new active timer at `at`.
    ///
    /// Fails with [`DomainError::AlreadyActive`] if a timer is running; the
    /// caller must [`Logbook::switch`] instead. `project_id` may be `None`
    /// (unassigned time stays visible for later classification).
    pub fn start_timer(
        &mut self,
        at: DateTime<Utc>,
        project_id: Option<Uuid>,
        tag_ids: Vec<Uuid>,
    ) -> Result<Uuid, DomainError> {
        if let Some(id) = self.active_span_id {
            return Err(DomainError::AlreadyActive(id));
        }
        self.check_project_ref(project_id)?;
        let tag_ids = self.normalize_tags(tag_ids)?;
        self.check_overlap(at, None, None)?;
        let span = Span::new(at, project_id, tag_ids);
        let id = span.id;
        self.spans.insert(id, span);
        self.active_span_id = Some(id);
        Ok(id)
    }

    /// Stop the active timer at `at`.
    ///
    /// Fails with [`DomainError::NoActiveTimer`] if no timer is running.
    pub fn stop_timer(&mut self, at: DateTime<Utc>) -> Result<Uuid, DomainError> {
        let id = self.active_span_id.ok_or(DomainError::NoActiveTimer)?;
        let span = self
            .spans
            .get(&id)
            .ok_or(DomainError::InconsistentActiveSpan)?;
        if at < span.started_at {
            return Err(DomainError::InvalidTimeRange);
        }
        self.check_overlap(span.started_at, Some(at), Some(id))?;
        let span = self
            .spans
            .get_mut(&id)
            .ok_or(DomainError::InconsistentActiveSpan)?;
        span.stopped_at = Some(at);
        span.updated_at = at;
        self.active_span_id = None;
        Ok(id)
    }

    /// Stop the active span (if any) and start a new one at the same instant.
    ///
    /// With nothing active this degenerates to a plain start.
    pub fn switch(
        &mut self,
        at: DateTime<Utc>,
        project_id: Option<Uuid>,
        tag_ids: Vec<Uuid>,
    ) -> Result<Uuid, DomainError> {
        self.check_project_ref(project_id)?;
        let tag_ids = self.normalize_tags(tag_ids)?;
        if let Some(id) = self.active_span_id {
            let span = self
                .spans
                .get(&id)
                .ok_or(DomainError::InconsistentActiveSpan)?;
            if at < span.started_at {
                return Err(DomainError::InvalidTimeRange);
            }
            self.check_overlap(span.started_at, Some(at), Some(id))?;
            self.check_overlap(at, None, Some(id))?;
            let span = self
                .spans
                .get_mut(&id)
                .ok_or(DomainError::InconsistentActiveSpan)?;
            span.stopped_at = Some(at);
            span.updated_at = at;
        } else {
            self.check_overlap(at, None, None)?;
        }
        let span = Span::new(at, project_id, tag_ids);
        let id = span.id;
        self.spans.insert(id, span);
        self.active_span_id = Some(id);
        Ok(id)
    }

    /// Apply one focused edit to a span. `None` leaves a field unchanged.
    ///
    /// `stopped_at` may only be changed on an already-stopped span (to correct
    /// a recorded stop time); it can never be set on the active span or
    /// cleared — stopping goes through [`Logbook::stop_timer`] or
    /// [`Logbook::switch`]. To clear a span's project use
    /// [`Logbook::unassign_span`].
    pub fn edit_span(
        &mut self,
        span_id: Uuid,
        at: DateTime<Utc>,
        started_at: Option<DateTime<Utc>>,
        stopped_at: Option<DateTime<Utc>>,
        project_id: Option<Uuid>,
        tag_ids: Option<Vec<Uuid>>,
    ) -> Result<(), DomainError> {
        self.check_project_ref(project_id)?;
        let new_tags = tag_ids.map(|ids| self.normalize_tags(ids)).transpose()?;
        let (new_started, new_stopped) = {
            let span = self
                .spans
                .get(&span_id)
                .ok_or(DomainError::SpanNotFound(span_id))?;
            if stopped_at.is_some() && span.stopped_at.is_none() {
                return Err(DomainError::InvalidStoppedAtEdit);
            }
            (
                started_at.unwrap_or(span.started_at),
                stopped_at.or(span.stopped_at),
            )
        };
        if let Some(stopped) = new_stopped {
            if new_started > stopped {
                return Err(DomainError::InvalidTimeRange);
            }
        }
        self.check_overlap(new_started, new_stopped, Some(span_id))?;
        let span = self
            .spans
            .get_mut(&span_id)
            .ok_or(DomainError::SpanNotFound(span_id))?;
        span.started_at = new_started;
        span.stopped_at = new_stopped;
        if let Some(project) = project_id {
            span.project_id = Some(project);
        }
        if let Some(tags) = new_tags {
            span.tag_ids = tags;
        }
        span.updated_at = at;
        Ok(())
    }

    /// Insert an already-completed span without touching the active timer.
    ///
    /// This is how a client records time that was not tracked live; unlike
    /// [`Logbook::start_timer`] it does not require the timer to be idle.
    pub fn add_span(
        &mut self,
        started_at: DateTime<Utc>,
        stopped_at: DateTime<Utc>,
        project_id: Option<Uuid>,
        tag_ids: Vec<Uuid>,
        at: DateTime<Utc>,
    ) -> Result<Uuid, DomainError> {
        self.check_project_ref(project_id)?;
        let tag_ids = self.normalize_tags(tag_ids)?;
        if started_at > stopped_at {
            return Err(DomainError::InvalidTimeRange);
        }
        self.check_overlap(started_at, Some(stopped_at), None)?;
        let mut span = Span::new(started_at, project_id, tag_ids);
        span.stopped_at = Some(stopped_at);
        span.created_at = at;
        span.updated_at = at;
        let id = span.id;
        self.spans.insert(id, span);
        Ok(id)
    }

    /// Remove a span entirely. Removing the active span discards the running
    /// timer and clears `active_span_id`; nothing references spans, so no
    /// other cleanup is needed.
    pub fn remove_span(&mut self, span_id: Uuid) -> Result<(), DomainError> {
        if self.spans.remove(&span_id).is_none() {
            return Err(DomainError::SpanNotFound(span_id));
        }
        if self.active_span_id == Some(span_id) {
            self.active_span_id = None;
        }
        Ok(())
    }

    /// Clear a span's project assignment. The time stays visible as
    /// unassigned for later classification.
    pub fn unassign_span(&mut self, span_id: Uuid, at: DateTime<Utc>) -> Result<(), DomainError> {
        let span = self
            .spans
            .get_mut(&span_id)
            .ok_or(DomainError::SpanNotFound(span_id))?;
        span.project_id = None;
        span.updated_at = at;
        Ok(())
    }

    /// Create a project. Names are case-insensitively unique, including
    /// archived records.
    pub fn add_project(
        &mut self,
        name: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Uuid, DomainError> {
        let name = name.into();
        self.check_project_name(&name, None)?;
        let project = Project::new(name, at);
        let id = project.id;
        self.projects.insert(id, project);
        Ok(id)
    }

    pub fn rename_project(
        &mut self,
        project_id: Uuid,
        name: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        let name = name.into();
        self.check_project_name(&name, Some(project_id))?;
        let project = self
            .projects
            .get_mut(&project_id)
            .ok_or(DomainError::ProjectNotFound(project_id))?;
        project.name = name;
        project.updated_at = at;
        Ok(())
    }

    /// Archive or unarchive a project. History is preserved; spans keep
    /// referring to the project by ID.
    pub fn set_project_archived(
        &mut self,
        project_id: Uuid,
        archived: bool,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        let project = self
            .projects
            .get_mut(&project_id)
            .ok_or(DomainError::ProjectNotFound(project_id))?;
        project.archived = archived;
        project.updated_at = at;
        Ok(())
    }

    /// Create a tag. Names are case-insensitively unique, including archived
    /// records.
    pub fn add_tag(
        &mut self,
        name: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Uuid, DomainError> {
        let name = name.into();
        self.check_tag_name(&name, None)?;
        let tag = Tag::new(name, at);
        let id = tag.id;
        self.tags.insert(id, tag);
        Ok(id)
    }

    pub fn rename_tag(
        &mut self,
        tag_id: Uuid,
        name: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        let name = name.into();
        self.check_tag_name(&name, Some(tag_id))?;
        let tag = self
            .tags
            .get_mut(&tag_id)
            .ok_or(DomainError::TagNotFound(tag_id))?;
        tag.name = name;
        tag.updated_at = at;
        Ok(())
    }

    /// Archive or unarchive a tag. History is preserved; spans keep referring
    /// to the tag by ID.
    pub fn set_tag_archived(
        &mut self,
        tag_id: Uuid,
        archived: bool,
        at: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        let tag = self
            .tags
            .get_mut(&tag_id)
            .ok_or(DomainError::TagNotFound(tag_id))?;
        tag.archived = archived;
        tag.updated_at = at;
        Ok(())
    }
}

fn find_duplicate<'a>(names: impl Iterator<Item = &'a String>) -> Option<String> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !seen.insert(name.to_lowercase()) {
            return Some(name.clone());
        }
    }
    None
}

#[pymethods]
impl Logbook {
    #[new]
    fn py_new() -> Self {
        Self::new()
    }

    #[getter]
    fn active_span_id(&self) -> Option<String> {
        self.active_span_id.map(|id| id.to_string())
    }

    /// The span currently recording time, if any.
    #[pyo3(name = "active_span")]
    fn py_active_span(&self) -> Option<Span> {
        self.active_span().cloned()
    }

    fn project(&self, id: String) -> PyResult<Option<Project>> {
        Ok(self.projects.get(&parse_uuid(&id)?).cloned())
    }

    fn tag(&self, id: String) -> PyResult<Option<Tag>> {
        Ok(self.tags.get(&parse_uuid(&id)?).cloned())
    }

    fn span(&self, id: String) -> PyResult<Option<Span>> {
        Ok(self.spans.get(&parse_uuid(&id)?).cloned())
    }

    fn projects(&self) -> Vec<Project> {
        self.projects.values().cloned().collect()
    }

    fn tags(&self) -> Vec<Tag> {
        self.tags.values().cloned().collect()
    }

    fn spans(&self) -> Vec<Span> {
        self.spans.values().cloned().collect()
    }

    #[pyo3(name = "start_timer", signature = (at, project_id=None, tag_ids=None))]
    fn py_start_timer(
        &mut self,
        at: DateTime<Utc>,
        project_id: Option<String>,
        tag_ids: Option<Vec<String>>,
    ) -> PyResult<String> {
        let id = self.start_timer(at, parse_uuid_opt(project_id)?, parse_uuid_vec(tag_ids)?)?;
        Ok(id.to_string())
    }

    #[pyo3(name = "stop_timer")]
    fn py_stop_timer(&mut self, at: DateTime<Utc>) -> PyResult<String> {
        Ok(self.stop_timer(at)?.to_string())
    }

    #[pyo3(name = "switch", signature = (at, project_id=None, tag_ids=None))]
    fn py_switch(
        &mut self,
        at: DateTime<Utc>,
        project_id: Option<String>,
        tag_ids: Option<Vec<String>>,
    ) -> PyResult<String> {
        let id = self.switch(at, parse_uuid_opt(project_id)?, parse_uuid_vec(tag_ids)?)?;
        Ok(id.to_string())
    }

    #[pyo3(
        name = "edit_span",
        signature = (span_id, at, started_at=None, stopped_at=None, project_id=None, tag_ids=None)
    )]
    fn py_edit_span(
        &mut self,
        span_id: String,
        at: DateTime<Utc>,
        started_at: Option<DateTime<Utc>>,
        stopped_at: Option<DateTime<Utc>>,
        project_id: Option<String>,
        tag_ids: Option<Vec<String>>,
    ) -> PyResult<()> {
        Ok(self.edit_span(
            parse_uuid(&span_id)?,
            at,
            started_at,
            stopped_at,
            parse_uuid_opt(project_id)?,
            tag_ids.map(|ids| parse_uuid_vec(Some(ids))).transpose()?,
        )?)
    }

    #[pyo3(
        name = "add_span",
        signature = (started_at, stopped_at, project_id=None, tag_ids=None, at=None)
    )]
    fn py_add_span(
        &mut self,
        started_at: DateTime<Utc>,
        stopped_at: DateTime<Utc>,
        project_id: Option<String>,
        tag_ids: Option<Vec<String>>,
        at: Option<DateTime<Utc>>,
    ) -> PyResult<String> {
        let id = self.add_span(
            started_at,
            stopped_at,
            parse_uuid_opt(project_id)?,
            parse_uuid_vec(tag_ids)?,
            at.unwrap_or_else(Utc::now),
        )?;
        Ok(id.to_string())
    }

    #[pyo3(name = "remove_span")]
    fn py_remove_span(&mut self, span_id: String) -> PyResult<()> {
        Ok(self.remove_span(parse_uuid(&span_id)?)?)
    }

    #[pyo3(name = "unassign_span")]
    fn py_unassign_span(&mut self, span_id: String, at: DateTime<Utc>) -> PyResult<()> {
        Ok(self.unassign_span(parse_uuid(&span_id)?, at)?)
    }

    #[pyo3(name = "add_project")]
    fn py_add_project(&mut self, name: String, at: DateTime<Utc>) -> PyResult<String> {
        Ok(self.add_project(name, at)?.to_string())
    }

    #[pyo3(name = "rename_project")]
    fn py_rename_project(
        &mut self,
        project_id: String,
        name: String,
        at: DateTime<Utc>,
    ) -> PyResult<()> {
        Ok(self.rename_project(parse_uuid(&project_id)?, name, at)?)
    }

    #[pyo3(name = "set_project_archived")]
    fn py_set_project_archived(
        &mut self,
        project_id: String,
        archived: bool,
        at: DateTime<Utc>,
    ) -> PyResult<()> {
        Ok(self.set_project_archived(parse_uuid(&project_id)?, archived, at)?)
    }

    #[pyo3(name = "add_tag")]
    fn py_add_tag(&mut self, name: String, at: DateTime<Utc>) -> PyResult<String> {
        Ok(self.add_tag(name, at)?.to_string())
    }

    #[pyo3(name = "rename_tag")]
    fn py_rename_tag(&mut self, tag_id: String, name: String, at: DateTime<Utc>) -> PyResult<()> {
        Ok(self.rename_tag(parse_uuid(&tag_id)?, name, at)?)
    }

    #[pyo3(name = "set_tag_archived")]
    fn py_set_tag_archived(
        &mut self,
        tag_id: String,
        archived: bool,
        at: DateTime<Utc>,
    ) -> PyResult<()> {
        Ok(self.set_tag_archived(parse_uuid(&tag_id)?, archived, at)?)
    }

    /// Serialize to the canonical pretty-printed JSON representation.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(self).map_err(|e| PyValueError::new_err(e.to_string()))
    }

    /// Parse a logbook from its canonical JSON representation.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        serde_json::from_str(json).map_err(|e| PyValueError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "Logbook(revision={}, projects={}, tags={}, spans={}, active_span_id={:?})",
            self.revision,
            self.projects.len(),
            self.tags.len(),
            self.spans.len(),
            self.active_span_id,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn start_then_stop() {
        let mut logbook = Logbook::new();
        let id = logbook.start_timer(at(0), None, vec![]).unwrap();
        assert_eq!(logbook.active_span_id, Some(id));
        assert!(logbook.active_span().unwrap().is_active());

        let stopped = logbook.stop_timer(at(60)).unwrap();
        assert_eq!(stopped, id);
        assert_eq!(logbook.active_span_id, None);
        assert_eq!(logbook.spans[&id].stopped_at, Some(at(60)));
        logbook.validate().unwrap();
    }

    #[test]
    fn start_while_active_errors() {
        let mut logbook = Logbook::new();
        logbook.start_timer(at(0), None, vec![]).unwrap();
        assert!(matches!(
            logbook.start_timer(at(10), None, vec![]),
            Err(DomainError::AlreadyActive(_))
        ));
    }

    #[test]
    fn stop_without_active_errors() {
        let mut logbook = Logbook::new();
        assert!(matches!(
            logbook.stop_timer(at(0)),
            Err(DomainError::NoActiveTimer)
        ));
    }

    #[test]
    fn switch_stops_and_starts_at_same_instant() {
        let mut logbook = Logbook::new();
        let project = logbook.add_project("work", at(0)).unwrap();
        let tag = logbook.add_tag("focus", at(0)).unwrap();
        let first = logbook.start_timer(at(0), Some(project), vec![]).unwrap();
        let second = logbook.switch(at(30), None, vec![tag]).unwrap();

        assert_eq!(logbook.spans[&first].stopped_at, Some(at(30)));
        assert_eq!(logbook.spans[&second].started_at, at(30));
        assert_eq!(logbook.spans[&second].tag_ids, vec![tag]);
        assert_eq!(logbook.active_span_id, Some(second));
        logbook.validate().unwrap();
    }

    #[test]
    fn switch_without_active_starts() {
        let mut logbook = Logbook::new();
        let id = logbook.switch(at(5), None, vec![]).unwrap();
        assert_eq!(logbook.active_span_id, Some(id));
        assert!(logbook.spans[&id].is_active());
    }

    #[test]
    fn switch_rejects_stop_before_start() {
        let mut logbook = Logbook::new();
        logbook.start_timer(at(100), None, vec![]).unwrap();
        assert!(matches!(
            logbook.switch(at(50), None, vec![]),
            Err(DomainError::InvalidTimeRange)
        ));
        assert!(logbook.active_span().is_some());
    }

    #[test]
    fn edit_span_updates_fields() {
        let mut logbook = Logbook::new();
        let project = logbook.add_project("work", at(0)).unwrap();
        let other = logbook.add_project("play", at(0)).unwrap();
        let tag = logbook.add_tag("x", at(0)).unwrap();
        let id = logbook.start_timer(at(0), Some(project), vec![]).unwrap();
        logbook.stop_timer(at(100)).unwrap();

        logbook
            .edit_span(
                id,
                at(200),
                Some(at(10)),
                Some(at(90)),
                Some(other),
                Some(vec![tag]),
            )
            .unwrap();
        let span = &logbook.spans[&id];
        assert_eq!(span.started_at, at(10));
        assert_eq!(span.stopped_at, Some(at(90)));
        assert_eq!(span.project_id, Some(other));
        assert_eq!(span.tag_ids, vec![tag]);
        logbook.validate().unwrap();
    }

    #[test]
    fn edit_span_cannot_stop_active_span() {
        let mut logbook = Logbook::new();
        let id = logbook.start_timer(at(0), None, vec![]).unwrap();
        assert!(matches!(
            logbook.edit_span(id, at(10), None, Some(at(50)), None, None),
            Err(DomainError::InvalidStoppedAtEdit)
        ));
        assert!(logbook.spans[&id].is_active());
    }

    #[test]
    fn edit_span_rejects_inverted_range() {
        let mut logbook = Logbook::new();
        let id = logbook.start_timer(at(0), None, vec![]).unwrap();
        logbook.stop_timer(at(100)).unwrap();
        assert!(matches!(
            logbook.edit_span(id, at(200), Some(at(150)), None, None, None),
            Err(DomainError::InvalidTimeRange)
        ));
    }

    #[test]
    fn add_span_records_completed_span_without_touching_active() {
        let mut logbook = Logbook::new();
        let project = logbook.add_project("work", at(0)).unwrap();
        let active = logbook.start_timer(at(500), Some(project), vec![]).unwrap();

        let added = logbook
            .add_span(at(0), at(100), Some(project), vec![], at(600))
            .unwrap();
        assert_eq!(logbook.spans[&added].stopped_at, Some(at(100)));
        assert_eq!(logbook.active_span_id, Some(active));

        assert!(matches!(
            logbook.add_span(at(200), at(100), None, vec![], at(600)),
            Err(DomainError::InvalidTimeRange)
        ));
        assert!(matches!(
            logbook.add_span(at(0), at(100), Some(Uuid::new_v4()), vec![], at(600)),
            Err(DomainError::ProjectNotFound(_))
        ));
        logbook.validate().unwrap();
    }

    #[test]
    fn add_span_rejects_overlapping_spans() {
        let mut logbook = Logbook::new();
        logbook
            .add_span(at(0), at(100), None, vec![], at(0))
            .unwrap();
        for (start, end) in [(50, 150), (10, 50), (0, 200)] {
            assert!(
                matches!(
                    logbook.add_span(at(start), at(end), None, vec![], at(0)),
                    Err(DomainError::Overlap(_))
                ),
                "span {start}–{end} must be rejected as overlapping"
            );
        }
        // Touching boundaries do not overlap.
        logbook
            .add_span(at(100), at(200), None, vec![], at(0))
            .unwrap();
        logbook
            .add_span(at(300), at(400), None, vec![], at(0))
            .unwrap();
        logbook.validate().unwrap();
    }

    #[test]
    fn add_span_rejects_overlap_with_active_timer() {
        let mut logbook = Logbook::new();
        logbook.start_timer(at(100), None, vec![]).unwrap();
        // The running timer occupies everything from its start onward.
        assert!(matches!(
            logbook.add_span(at(150), at(200), None, vec![], at(150)),
            Err(DomainError::Overlap(_))
        ));
        // A span ending exactly at the timer's start is fine.
        logbook
            .add_span(at(0), at(100), None, vec![], at(150))
            .unwrap();
        logbook.validate().unwrap();
    }

    #[test]
    fn edit_span_rejects_overlap() {
        let mut logbook = Logbook::new();
        let a = logbook
            .add_span(at(0), at(100), None, vec![], at(0))
            .unwrap();
        let b = logbook
            .add_span(at(200), at(300), None, vec![], at(0))
            .unwrap();
        assert!(matches!(
            logbook.edit_span(b, at(400), Some(at(50)), None, None, None),
            Err(DomainError::Overlap(_))
        ));
        assert!(matches!(
            logbook.edit_span(a, at(400), None, Some(at(250)), None, None),
            Err(DomainError::Overlap(_))
        ));
        // Moving b's start exactly to a's end is allowed, and a span never
        // overlaps itself.
        logbook
            .edit_span(b, at(400), Some(at(100)), None, None, None)
            .unwrap();
        logbook
            .edit_span(a, at(400), Some(at(10)), Some(at(90)), None, None)
            .unwrap();
        logbook.validate().unwrap();
    }

    #[test]
    fn start_timer_rejects_start_before_a_recorded_span_ends() {
        let mut logbook = Logbook::new();
        logbook
            .add_span(at(100), at(200), None, vec![], at(0))
            .unwrap();
        assert!(matches!(
            logbook.start_timer(at(150), None, vec![]),
            Err(DomainError::Overlap(_))
        ));
        // Starting exactly when the recorded span ends is fine.
        logbook.start_timer(at(200), None, vec![]).unwrap();
        logbook.validate().unwrap();
    }

    #[test]
    fn stop_timer_rejects_overlap_with_recorded_span() {
        let mut logbook = Logbook::new();
        let active = logbook.start_timer(at(0), None, vec![]).unwrap();
        // Inject a span overlapping the timer's future, bypassing
        // add_span's check: the mutation must still enforce the invariant.
        let mut stray = Span::new(at(50), None, vec![]);
        stray.stopped_at = Some(at(100));
        logbook.spans.insert(stray.id, stray);
        assert!(matches!(
            logbook.stop_timer(at(150)),
            Err(DomainError::Overlap(_))
        ));
        assert_eq!(logbook.active_span_id, Some(active));
        assert!(logbook.active_span().unwrap().is_active());
    }

    #[test]
    fn switch_rejects_overlap_with_recorded_span() {
        // The new timer may not run into recorded time.
        let mut logbook = Logbook::new();
        logbook
            .add_span(at(100), at(200), None, vec![], at(0))
            .unwrap();
        assert!(matches!(
            logbook.switch(at(150), None, vec![]),
            Err(DomainError::Overlap(_))
        ));
        logbook.switch(at(200), None, vec![]).unwrap();

        // Stopping the old timer may not overlap a recorded span either.
        let mut logbook = Logbook::new();
        logbook.start_timer(at(0), None, vec![]).unwrap();
        let mut stray = Span::new(at(50), None, vec![]);
        stray.stopped_at = Some(at(100));
        logbook.spans.insert(stray.id, stray);
        assert!(matches!(
            logbook.switch(at(150), None, vec![]),
            Err(DomainError::Overlap(_))
        ));
        assert!(logbook.active_span().unwrap().is_active());
    }

    #[test]
    fn validate_rejects_overlapping_spans() {
        let mut logbook = Logbook::new();
        let a = logbook
            .add_span(at(0), at(100), None, vec![], at(0))
            .unwrap();
        let b = logbook
            .add_span(at(200), at(300), None, vec![], at(0))
            .unwrap();
        logbook.validate().unwrap();

        logbook.spans.get_mut(&b).unwrap().started_at = at(50);
        assert!(matches!(logbook.validate(), Err(DomainError::Overlap(_))));

        // The active span runs unbounded: a later span overlaps it.
        logbook.spans.get_mut(&b).unwrap().started_at = at(200);
        logbook.spans.get_mut(&a).unwrap().stopped_at = None;
        logbook.active_span_id = Some(a);
        assert!(matches!(logbook.validate(), Err(DomainError::Overlap(_))));
    }

    #[test]
    fn remove_span_discards_active_timer() {
        let mut logbook = Logbook::new();
        let active = logbook.start_timer(at(0), None, vec![]).unwrap();
        logbook.remove_span(active).unwrap();
        assert_eq!(logbook.active_span_id, None);
        assert!(logbook.spans.is_empty());
        logbook.validate().unwrap();
    }

    #[test]
    fn remove_span_keeps_other_spans_and_active() {
        let mut logbook = Logbook::new();
        let first = logbook.start_timer(at(0), None, vec![]).unwrap();
        let second = logbook.switch(at(10), None, vec![]).unwrap();
        logbook.remove_span(first).unwrap();
        assert!(!logbook.spans.contains_key(&first));
        assert_eq!(logbook.active_span_id, Some(second));
        logbook.validate().unwrap();
    }

    #[test]
    fn remove_span_unknown_errors() {
        let mut logbook = Logbook::new();
        assert!(matches!(
            logbook.remove_span(Uuid::new_v4()),
            Err(DomainError::SpanNotFound(_))
        ));
    }

    #[test]
    fn unassign_span_clears_project() {
        let mut logbook = Logbook::new();
        let project = logbook.add_project("work", at(0)).unwrap();
        let id = logbook.start_timer(at(0), Some(project), vec![]).unwrap();
        logbook.stop_timer(at(10)).unwrap();
        logbook.unassign_span(id, at(20)).unwrap();
        assert_eq!(logbook.spans[&id].project_id, None);
    }

    #[test]
    fn project_names_unique_case_insensitive() {
        let mut logbook = Logbook::new();
        let work = logbook.add_project("Work", at(0)).unwrap();
        assert!(matches!(
            logbook.add_project("work", at(0)),
            Err(DomainError::DuplicateProjectName(_))
        ));
        let personal = logbook.add_project("Personal", at(0)).unwrap();
        assert!(matches!(
            logbook.rename_project(personal, "WORK", at(1)),
            Err(DomainError::DuplicateProjectName(_))
        ));
        // Renaming to the same name is fine.
        logbook.rename_project(work, "Work", at(1)).unwrap();
        logbook.rename_project(work, "Work 2", at(1)).unwrap();
    }

    #[test]
    fn tag_names_unique_case_insensitive() {
        let mut logbook = Logbook::new();
        logbook.add_tag("Focus", at(0)).unwrap();
        assert!(matches!(
            logbook.add_tag("FOCUS", at(0)),
            Err(DomainError::DuplicateTagName(_))
        ));
    }

    #[test]
    fn archived_projects_still_block_names() {
        let mut logbook = Logbook::new();
        let id = logbook.add_project("Work", at(0)).unwrap();
        logbook.set_project_archived(id, true, at(1)).unwrap();
        assert!(matches!(
            logbook.add_project("work", at(2)),
            Err(DomainError::DuplicateProjectName(_))
        ));
    }

    #[test]
    fn validate_rejects_inconsistent_active_span() {
        let mut logbook = Logbook::new();
        let id = logbook.start_timer(at(0), None, vec![]).unwrap();

        // active_span_id pointing at a stopped span is corrupt.
        logbook.spans.get_mut(&id).unwrap().stopped_at = Some(at(10));
        assert!(matches!(
            logbook.validate(),
            Err(DomainError::InconsistentActiveSpan)
        ));

        // An unstopped span with no active_span_id is corrupt.
        logbook.active_span_id = None;
        logbook.spans.get_mut(&id).unwrap().stopped_at = None;
        assert!(matches!(
            logbook.validate(),
            Err(DomainError::InconsistentActiveSpan)
        ));
    }

    #[test]
    fn validate_rejects_dangling_references() {
        let mut logbook = Logbook::new();
        let id = logbook.start_timer(at(0), None, vec![]).unwrap();
        logbook.stop_timer(at(10)).unwrap();
        logbook.spans.get_mut(&id).unwrap().project_id = Some(Uuid::new_v4());
        assert!(matches!(
            logbook.validate(),
            Err(DomainError::ProjectNotFound(_))
        ));
        logbook.spans.get_mut(&id).unwrap().project_id = None;
        logbook.spans.get_mut(&id).unwrap().tag_ids = vec![Uuid::new_v4()];
        assert!(matches!(
            logbook.validate(),
            Err(DomainError::TagNotFound(_))
        ));
    }

    #[test]
    fn validate_rejects_unsupported_schema_version() {
        let mut logbook = Logbook::new();
        logbook.schema_version = 99;
        assert!(matches!(
            logbook.validate(),
            Err(DomainError::UnsupportedSchemaVersion(99))
        ));
    }

    #[test]
    fn missing_references_rejected_by_mutations() {
        let mut logbook = Logbook::new();
        let bogus = Uuid::new_v4();
        assert!(matches!(
            logbook.start_timer(at(0), Some(bogus), vec![]),
            Err(DomainError::ProjectNotFound(_))
        ));
        assert!(matches!(
            logbook.start_timer(at(0), None, vec![bogus]),
            Err(DomainError::TagNotFound(_))
        ));
        assert_eq!(logbook.active_span_id, None);
    }

    #[test]
    fn logbook_json_roundtrip() {
        let mut logbook = Logbook::new();
        let project = logbook.add_project("work", at(0)).unwrap();
        let tag = logbook.add_tag("focus", at(0)).unwrap();
        logbook
            .start_timer(at(10), Some(project), vec![tag])
            .unwrap();
        let json = serde_json::to_string(&logbook).unwrap();
        let parsed: Logbook = serde_json::from_str(&json).unwrap();
        assert_eq!(logbook, parsed);
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("\"active_span_id\""));
    }
}
