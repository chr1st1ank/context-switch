//! Client-side envelope encryption implementation using standard `age`.

use age::secrecy::SecretString;
use std::io::{Read, Write};
use thiserror::Error;

/// Cryptographic errors.
#[derive(Debug, Error, Clone)]
pub enum CryptoError {
    #[error("Decryption failed (wrong passphrase or tampered data)")]
    DecryptionFailed,
    #[error("Invalid envelope header: {0}")]
    InvalidHeader(String),
    #[error("Internal cryptographic error: {0}")]
    Internal(String),
}

pub trait Cipher: Send + Sync {
    /// Decrypt an envelope given a passphrase. Returns the decrypted plaintext.
    fn open(&self, envelope: &[u8], passphrase: &str) -> Result<Vec<u8>, CryptoError>;

    /// Encrypt a plaintext given a passphrase.
    fn seal(&self, plaintext: &[u8], passphrase: &str) -> Result<Vec<u8>, CryptoError>;
}

pub struct IdentityCipher;

impl Cipher for IdentityCipher {
    fn open(&self, envelope: &[u8], _passphrase: &str) -> Result<Vec<u8>, CryptoError> {
        Ok(envelope.to_vec())
    }

    fn seal(&self, plaintext: &[u8], _passphrase: &str) -> Result<Vec<u8>, CryptoError> {
        Ok(plaintext.to_vec())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnvelopeCipher {
    work_factor: u8,
}

impl Default for EnvelopeCipher {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvelopeCipher {
    /// Default scrypt work factor log2(N) = 16 for interactive CLI performance (~20–50ms).
    pub const DEFAULT_WORK_FACTOR: u8 = 16;

    pub fn new() -> Self {
        Self {
            work_factor: Self::DEFAULT_WORK_FACTOR,
        }
    }

    pub fn with_work_factor(work_factor: u8) -> Self {
        Self { work_factor }
    }
}

impl Cipher for EnvelopeCipher {
    fn open(&self, envelope: &[u8], passphrase: &str) -> Result<Vec<u8>, CryptoError> {
        let decryptor = age::Decryptor::new_buffered(envelope)
            .map_err(|e| CryptoError::InvalidHeader(e.to_string()))?;

        if !decryptor.is_scrypt() {
            return Err(CryptoError::InvalidHeader(
                "not a passphrase-encrypted age envelope".to_string(),
            ));
        }

        let secret = SecretString::from(passphrase.to_string());
        let identity = age::scrypt::Identity::new(secret);

        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .map_err(|e| match e {
                age::DecryptError::DecryptionFailed
                | age::DecryptError::KeyDecryptionFailed
                | age::DecryptError::NoMatchingKeys => CryptoError::DecryptionFailed,
                other => CryptoError::InvalidHeader(other.to_string()),
            })?;

        let mut plaintext = Vec::new();
        reader
            .read_to_end(&mut plaintext)
            .map_err(|_| CryptoError::DecryptionFailed)?;
        Ok(plaintext)
    }

    fn seal(&self, plaintext: &[u8], passphrase: &str) -> Result<Vec<u8>, CryptoError> {
        let secret = SecretString::from(passphrase.to_string());
        let mut recipient = age::scrypt::Recipient::new(secret);
        recipient.set_work_factor(self.work_factor);

        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                .map_err(|e| CryptoError::Internal(e.to_string()))?;

        let mut output = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut output)
            .map_err(|e| CryptoError::Internal(e.to_string()))?;

        writer
            .write_all(plaintext)
            .map_err(|e| CryptoError::Internal(e.to_string()))?;

        writer
            .finish()
            .map_err(|e| CryptoError::Internal(e.to_string()))?;

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_open_round_trip() {
        let cipher = EnvelopeCipher::with_work_factor(10);
        let msg = b"hello time tracking world";
        let sealed = cipher.seal(msg, "super-secret").unwrap();
        let opened = cipher.open(&sealed, "super-secret").unwrap();
        assert_eq!(opened, msg);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let cipher = EnvelopeCipher::with_work_factor(10);
        let msg = b"secret payload";
        let sealed = cipher.seal(msg, "correct-pass").unwrap();
        let err = cipher.open(&sealed, "wrong-pass").unwrap_err();
        assert!(matches!(err, CryptoError::DecryptionFailed));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let cipher = EnvelopeCipher::with_work_factor(10);
        let msg = b"secret payload";
        let mut sealed = cipher.seal(msg, "pass").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0x55;
        let err = cipher.open(&sealed, "pass").unwrap_err();
        assert!(matches!(err, CryptoError::DecryptionFailed));
    }

    #[test]
    fn invalid_header_fails() {
        let cipher = EnvelopeCipher::new();
        let err = cipher.open(b"not an age file", "pass").unwrap_err();
        assert!(matches!(err, CryptoError::InvalidHeader(_)));
    }
}
