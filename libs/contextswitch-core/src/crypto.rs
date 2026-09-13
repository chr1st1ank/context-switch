//! Client-side envelope encryption implementation.

use argon2::{Algorithm, Argon2, Params};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Cryptographic errors.
#[derive(Debug, Error, Clone)]
pub enum CryptoError {
    #[error("Decryption failed (wrong passphrase or tampered data)")]
    DecryptionFailed,
    #[error("Invalid envelope header: {0}")]
    InvalidHeader(String),
    #[error("Invalid Argon2 parameters: {0}")]
    InvalidParams(String),
    #[error("Internal cryptographic error: {0}")]
    Internal(String),
}

/// KeyState tracks the active key identity and the full wrapped-key list.
///
/// Each [`WrappedKey`] carries its own [`DerivationParams`], since different
/// wrapped copies of the master key may be wrapped under different
/// passphrases (and therefore different salts/costs) — see story 48.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyState {
    pub active_key_id: String,
    pub keys: Vec<WrappedKey>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DerivationParams {
    pub algorithm: String, // "argon2id"
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt: String, // base64-encoded
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WrappedKey {
    pub key_id: String,
    pub derivation: DerivationParams,
    pub nonce: String,         // base64-encoded, 24 bytes
    pub encrypted_key: String, // base64-encoded, ciphertext + 16-byte tag
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvelopeHeader {
    pub version: u32,
    pub method: String, // "xchacha20-poly1305"
    pub active_key_id: String,
    pub reserved_compression: u8,
    pub keys: Vec<WrappedKey>,
}

pub trait Cipher: Send + Sync {
    /// Decrypt an envelope given a passphrase. Returns the decrypted plaintext and the KeyState.
    fn open(&self, envelope: &[u8], passphrase: &str) -> Result<(Vec<u8>, KeyState), CryptoError>;

    /// Encrypt a plaintext given a passphrase and optional prior KeyState.
    /// If prior is None (first write), a fresh master key is generated and wrapped.
    /// If prior is Some, the existing master key is extracted using the passphrase and re-wrapped if needed,
    /// or just reused.
    fn seal(
        &self,
        plaintext: &[u8],
        passphrase: &str,
        prior: Option<&KeyState>,
    ) -> Result<Vec<u8>, CryptoError>;
}

pub struct IdentityCipher;

impl Cipher for IdentityCipher {
    fn open(&self, envelope: &[u8], _passphrase: &str) -> Result<(Vec<u8>, KeyState), CryptoError> {
        let dummy_state = KeyState {
            active_key_id: "identity".to_string(),
            keys: vec![],
        };
        Ok((envelope.to_vec(), dummy_state))
    }

    fn seal(
        &self,
        plaintext: &[u8],
        _passphrase: &str,
        _prior: Option<&KeyState>,
    ) -> Result<Vec<u8>, CryptoError> {
        Ok(plaintext.to_vec())
    }
}

pub struct EnvelopeCipher;

impl EnvelopeCipher {
    /// Recommended production parameters for Argon2id.
    pub fn default_params() -> Result<(Params, Vec<u8>), CryptoError> {
        let mut salt = vec![0u8; 16];
        getrandom::getrandom(&mut salt).map_err(|e| CryptoError::Internal(e.to_string()))?;
        // Memory-hard but reasonably fast for single user: 16MB, 3 iterations, 1 lane.
        let params = Params::new(16384, 3, 1, Some(32))
            .map_err(|e| CryptoError::InvalidParams(e.to_string()))?;
        Ok((params, salt))
    }

    fn derive_key(passphrase: &str, params: &DerivationParams) -> Result<[u8; 32], CryptoError> {
        let salt_bytes = BASE64
            .decode(&params.salt)
            .map_err(|e| CryptoError::InvalidHeader(format!("salt base64 decode failed: {e}")))?;

        let p = Params::new(params.m_cost, params.t_cost, params.p_cost, Some(32))
            .map_err(|e| CryptoError::InvalidParams(e.to_string()))?;

        let argon2 = Argon2::new(Algorithm::Argon2id, argon2::Version::V0x13, p);

        let mut kek = [0u8; 32];
        argon2
            .hash_password_into(passphrase.as_bytes(), &salt_bytes, &mut kek)
            .map_err(|e| CryptoError::Internal(format!("argon2 derive failed: {e}")))?;
        Ok(kek)
    }

    /// Decode a base64 24-byte XChaCha20-Poly1305 nonce, rejecting anything
    /// that isn't exactly the right length rather than panicking.
    fn decode_nonce(encoded: &str) -> Result<XNonce, CryptoError> {
        let bytes = BASE64
            .decode(encoded)
            .map_err(|e| CryptoError::InvalidHeader(format!("nonce base64 decode failed: {e}")))?;
        let arr: [u8; 24] = bytes.try_into().map_err(|v: Vec<u8>| {
            CryptoError::InvalidHeader(format!("nonce must be 24 bytes, got {}", v.len()))
        })?;
        Ok(XNonce::from(arr))
    }

    fn decrypt_master_key(passphrase: &str, state: &KeyState) -> Result<[u8; 32], CryptoError> {
        let wrapped = state
            .keys
            .iter()
            .find(|k| k.key_id == state.active_key_id)
            .ok_or_else(|| {
                CryptoError::InvalidHeader(format!(
                    "active key id {} not present in wrapped-key list",
                    state.active_key_id
                ))
            })?;

        let kek = Self::derive_key(passphrase, &wrapped.derivation)?;

        let nonce = Self::decode_nonce(&wrapped.nonce)?;
        let enc_bytes = BASE64.decode(&wrapped.encrypted_key).map_err(|e| {
            CryptoError::InvalidHeader(format!("wrapped key base64 decode failed: {e}"))
        })?;

        let cipher = XChaCha20Poly1305::new(&kek.into());

        // Associated data is the key_id
        let payload = Payload {
            msg: &enc_bytes,
            aad: wrapped.key_id.as_bytes(),
        };

        let master_key_vec = cipher
            .decrypt(&nonce, payload)
            .map_err(|_| CryptoError::DecryptionFailed)?;

        if master_key_vec.len() != 32 {
            return Err(CryptoError::DecryptionFailed);
        }

        let mut master_key = [0u8; 32];
        master_key.copy_from_slice(&master_key_vec);
        Ok(master_key)
    }
}

impl Cipher for EnvelopeCipher {
    fn open(&self, envelope: &[u8], passphrase: &str) -> Result<(Vec<u8>, KeyState), CryptoError> {
        if envelope.len() < 4 {
            return Err(CryptoError::InvalidHeader("envelope too short".to_string()));
        }
        if &envelope[0..4] != b"COSW" {
            return Err(CryptoError::InvalidHeader(
                "missing magic bytes".to_string(),
            ));
        }
        if envelope.len() < 8 {
            return Err(CryptoError::InvalidHeader(
                "envelope missing header size".to_string(),
            ));
        }
        let header_len = u32::from_le_bytes(envelope[4..8].try_into().unwrap()) as usize;
        if envelope.len() < 8 + header_len {
            return Err(CryptoError::InvalidHeader("envelope truncated".to_string()));
        }

        let header_json = &envelope[8..8 + header_len];
        let header: EnvelopeHeader = serde_json::from_slice(header_json)
            .map_err(|e| CryptoError::InvalidHeader(format!("header JSON parse failed: {e}")))?;

        if header.version != 1 {
            return Err(CryptoError::InvalidHeader(format!(
                "unsupported envelope version: {}",
                header.version
            )));
        }
        if header.method != "xchacha20-poly1305" {
            return Err(CryptoError::InvalidHeader(format!(
                "unsupported method: {}",
                header.method
            )));
        }
        if header.reserved_compression != 0 {
            return Err(CryptoError::InvalidHeader(format!(
                "unsupported compression method: {}",
                header.reserved_compression
            )));
        }

        let state = KeyState {
            active_key_id: header.active_key_id.clone(),
            keys: header.keys.clone(),
        };

        // Decrypt the master key
        let master_key = Self::decrypt_master_key(passphrase, &state)?;

        // Now decrypt the main payload
        let payload_offset = 8 + header_len;
        let encrypted_payload = &envelope[payload_offset..];
        if encrypted_payload.len() < 24 {
            return Err(CryptoError::InvalidHeader(
                "envelope missing payload nonce".to_string(),
            ));
        }
        let nonce_bytes: [u8; 24] = encrypted_payload[0..24].try_into().unwrap();
        let nonce = XNonce::from(nonce_bytes);
        let ciphertext = &encrypted_payload[24..];

        let cipher = XChaCha20Poly1305::new(&master_key.into());

        // Associated data is the JSON header
        let payload = Payload {
            msg: ciphertext,
            aad: header_json,
        };

        let plaintext = cipher
            .decrypt(&nonce, payload)
            .map_err(|_| CryptoError::DecryptionFailed)?;

        Ok((plaintext, state))
    }

    fn seal(
        &self,
        plaintext: &[u8],
        passphrase: &str,
        prior: Option<&KeyState>,
    ) -> Result<Vec<u8>, CryptoError> {
        let (master_key, mut state) = match prior {
            Some(p) => {
                let mk = Self::decrypt_master_key(passphrase, p)?;
                (mk, p.clone())
            }
            None => {
                // Generate a random master key
                let mut mk = [0u8; 32];
                getrandom::getrandom(&mut mk).map_err(|e| CryptoError::Internal(e.to_string()))?;

                let active_key_id = uuid::Uuid::new_v4().to_string();
                let state = KeyState {
                    active_key_id,
                    keys: vec![],
                };
                (mk, state)
            }
        };

        // If the active key is not wrapped in keys list (or we generated a new one), we wrap it.
        if !state.keys.iter().any(|k| k.key_id == state.active_key_id) {
            let (params, salt) = Self::default_params()?;
            let derivation = DerivationParams {
                algorithm: "argon2id".to_string(),
                m_cost: params.m_cost(),
                t_cost: params.t_cost(),
                p_cost: params.p_cost(),
                salt: BASE64.encode(&salt),
            };
            let kek = Self::derive_key(passphrase, &derivation)?;

            let mut wrapper_nonce = [0u8; 24];
            getrandom::getrandom(&mut wrapper_nonce)
                .map_err(|e| CryptoError::Internal(e.to_string()))?;

            let cipher = XChaCha20Poly1305::new(&kek.into());
            let nonce = XNonce::from(wrapper_nonce);

            let payload = Payload {
                msg: &master_key,
                aad: state.active_key_id.as_bytes(),
            };
            let enc_key = cipher
                .encrypt(&nonce, payload)
                .map_err(|e| CryptoError::Internal(e.to_string()))?;

            state.keys.push(WrappedKey {
                key_id: state.active_key_id.clone(),
                derivation,
                nonce: BASE64.encode(wrapper_nonce),
                encrypted_key: BASE64.encode(&enc_key),
            });
        }

        // Construct the Header
        let header = EnvelopeHeader {
            version: 1,
            method: "xchacha20-poly1305".to_string(),
            active_key_id: state.active_key_id,
            reserved_compression: 0,
            keys: state.keys,
        };

        let header_json =
            serde_json::to_vec(&header).map_err(|e| CryptoError::Internal(e.to_string()))?;

        // Encrypt the plaintext using the master key
        let mut main_nonce = [0u8; 24];
        getrandom::getrandom(&mut main_nonce).map_err(|e| CryptoError::Internal(e.to_string()))?;

        let cipher = XChaCha20Poly1305::new(&master_key.into());
        let nonce = XNonce::from(main_nonce);

        let payload = Payload {
            msg: plaintext,
            aad: &header_json,
        };
        let ciphertext = cipher
            .encrypt(&nonce, payload)
            .map_err(|e| CryptoError::Internal(e.to_string()))?;

        // Assembly: Magic (4B) + HeaderLen (4B, LE) + HeaderJSON + MainNonce (24B) + Ciphertext
        let mut envelope = Vec::new();
        envelope.extend_from_slice(b"COSW");
        let header_len = header_json.len() as u32;
        envelope.extend_from_slice(&header_len.to_le_bytes());
        envelope.extend_from_slice(&header_json);
        envelope.extend_from_slice(&main_nonce);
        envelope.extend_from_slice(&ciphertext);

        Ok(envelope)
    }
}
