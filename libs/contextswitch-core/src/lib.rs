//! contextswitch-core — shared domain model and storage provider interface.
//!
//! - [`domain`]: [`Span`](domain::Span), [`Project`](domain::Project),
//!   [`Tag`](domain::Tag), and the canonical [`Logbook`](domain::Logbook)
//!   with invariant-checked mutations.
//! - [`storage`]: the [`StorageProvider`](storage::StorageProvider) contract
//!   and [`LocalFsProvider`](storage::LocalFsProvider).
//! - [`conformance`]: the provider conformance suite every implementation
//!   must pass.
//!
//! The same types and rules are exposed to Python through the
//! `contextswitch_core` extension module when the `python` feature
//! (on by default) is enabled; other FFI consumers such as the UniFFI
//! Android bindings build with `default-features = false`.

#[cfg(feature = "python")]
use crate::crypto::Cipher;
#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
#[pyfunction]
fn decrypt_envelope(envelope: Vec<u8>, passphrase: String) -> PyResult<String> {
    let cipher = crate::crypto::EnvelopeCipher;
    let (plaintext, _) = cipher
        .open(&envelope, &passphrase)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let s = String::from_utf8(plaintext).map_err(|_| {
        pyo3::exceptions::PyValueError::new_err("decrypted data is not valid UTF-8")
    })?;
    Ok(s)
}

/// Seal plaintext into a fresh envelope under a new random master key.
/// The counterpart to [`decrypt_envelope`]; exposed mainly so Python tests
/// (and disaster-recovery tooling) can construct envelopes without a
/// network round trip through `S3Provider`.
#[cfg(feature = "python")]
#[pyfunction]
fn encrypt_envelope(plaintext: String, passphrase: String) -> PyResult<Vec<u8>> {
    let cipher = crate::crypto::EnvelopeCipher;
    cipher
        .seal(plaintext.as_bytes(), &passphrase, None)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

pub mod blob;
pub mod conformance;
pub mod crypto;
pub mod domain;
pub mod provider;
pub mod s3;
pub mod storage;

/// Python exception types raised by the bindings.
#[cfg(feature = "python")]
pub mod exceptions {
    use pyo3::create_exception;
    use pyo3::exceptions::PyException;

    create_exception!(
        contextswitch_core,
        DomainError,
        PyException,
        "A domain rule or logbook invariant was violated."
    );
    create_exception!(
        contextswitch_core,
        StorageError,
        PyException,
        "A storage provider operation failed."
    );
}

/// Python module initialization.
#[cfg(feature = "python")]
#[pymodule]
fn contextswitch_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<domain::Project>()?;
    m.add_class::<domain::Tag>()?;
    m.add_class::<domain::Span>()?;
    m.add_class::<domain::Logbook>()?;
    m.add_class::<storage::StorageSnapshot>()?;
    m.add_class::<storage::LocalFsProvider>()?;
    m.add_class::<storage::S3Provider>()?;
    m.add_function(wrap_pyfunction!(decrypt_envelope, m)?)?;
    m.add_function(wrap_pyfunction!(encrypt_envelope, m)?)?;
    m.add("DomainError", m.py().get_type::<exceptions::DomainError>())?;
    m.add(
        "StorageError",
        m.py().get_type::<exceptions::StorageError>(),
    )?;
    Ok(())
}
