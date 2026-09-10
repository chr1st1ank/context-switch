/// Domain model for context-switch
///
/// Core types: Span, Project, Tag
///
/// TODO: Implement Rust domain types with PyO3 bindings
/// - Project: named work context with stable ID
/// - Tag: reusable label for spans
/// - Span: mutable time record with start, optional stop, optional project, tags
///
/// Each type should have:
/// - Rust struct with serde serialization
/// - PyO3 wrapper class for Python interop
/// - Methods for common operations (e.g., Span::is_active)

// Placeholder for future implementation
pub struct Project;
pub struct PyProject;

pub struct Tag;
pub struct PyTag;

pub struct Span;
pub struct PySpan;
