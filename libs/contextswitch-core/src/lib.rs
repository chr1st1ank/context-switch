//! contextswitch-core — shared domain model and storage provider interface.
//!
//! - [`domain`]: [`Span`](domain::Span), [`Project`](domain::Project),
//!   [`Tag`](domain::Tag), and the canonical [`Document`](domain::Document)
//!   with invariant-checked mutations.
//! - [`storage`]: the [`StorageProvider`](storage::StorageProvider) contract
//!   and [`LocalFsProvider`](storage::LocalFsProvider).
//! - [`conformance`]: the provider conformance suite every implementation
//!   must pass.
//!
//! The same types and rules are exposed to Python through the
//! `contextswitch_core` extension module.

use pyo3::prelude::*;

pub mod conformance;
pub mod domain;
pub mod storage;

/// Python exception types raised by the bindings.
pub mod exceptions {
    use pyo3::create_exception;
    use pyo3::exceptions::PyException;

    create_exception!(
        contextswitch_core,
        DomainError,
        PyException,
        "A domain rule or document invariant was violated."
    );
    create_exception!(
        contextswitch_core,
        StorageError,
        PyException,
        "A storage provider operation failed."
    );
}

/// Python module initialization.
#[pymodule]
fn contextswitch_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<domain::Project>()?;
    m.add_class::<domain::Tag>()?;
    m.add_class::<domain::Span>()?;
    m.add_class::<domain::Document>()?;
    m.add_class::<storage::StorageSnapshot>()?;
    m.add_class::<storage::LocalFsProvider>()?;
    m.add("DomainError", m.py().get_type::<exceptions::DomainError>())?;
    m.add(
        "StorageError",
        m.py().get_type::<exceptions::StorageError>(),
    )?;
    Ok(())
}
