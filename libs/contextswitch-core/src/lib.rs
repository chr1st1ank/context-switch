/// contextswitch-core — Shared domain model and storage provider interface
///
/// This library provides the core domain types and storage interface for context-switch,
/// with Python bindings via PyO3.
///
/// TODO: Complete implementation
/// - Implement domain types in domain.rs
/// - Implement storage interface in storage.rs
/// - Set up PyO3 module initialization
/// - Add comprehensive tests
pub mod domain;
pub mod storage;

// TODO: PyO3 module initialization
// #[pymodule]
// fn contextswitch_core(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
//     m.add_class::<domain::PySpan>()?;
//     m.add_class::<domain::PyProject>()?;
//     m.add_class::<domain::PyTag>()?;
//     Ok(())
// }
