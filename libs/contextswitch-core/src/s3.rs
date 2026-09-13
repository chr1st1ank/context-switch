//! S3-compatible BlobStore implementation with AWS V4 Signature signing.

use chrono::Utc;
use std::path::PathBuf;

use crate::blob::{BlobStore, Precondition};
use crate::storage::StorageError;

#[derive(Debug, Clone)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

pub fn load_aws_credentials(profile: Option<&str>) -> Result<AwsCredentials, StorageError> {
    // 1. Try environment variables first.
    if let (Ok(key), Ok(secret)) = (
        std::env::var("AWS_ACCESS_KEY_ID"),
        std::env::var("AWS_SECRET_ACCESS_KEY"),
    ) {
        let token = std::env::var("AWS_SESSION_TOKEN").ok();
        return Ok(AwsCredentials {
            access_key_id: key,
            secret_access_key: secret,
            session_token: token,
        });
    }

    // 2. Try shared credentials file.
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("USERPROFILE").map(PathBuf::from))
        .map_err(|_| {
            StorageError::Unauthorized(
                "AWS credentials not found: HOME/USERPROFILE env var missing".to_string(),
            )
        })?;

    let cred_path = home.join(".aws").join("credentials");
    if !cred_path.exists() {
        return Err(StorageError::Unauthorized("AWS credentials not found: environment variables not set and ~/.aws/credentials does not exist".to_string()));
    }

    let content = std::fs::read_to_string(&cred_path).map_err(|e| {
        StorageError::Unauthorized(format!("failed to read ~/.aws/credentials: {e}"))
    })?;

    let target_profile = profile.unwrap_or("default");
    let mut current_profile = String::new();
    let mut access_key_id = None;
    let mut secret_access_key = None;
    let mut session_token = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_profile = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        if current_profile == target_profile {
            if let Some(pos) = line.find('=') {
                let key = line[..pos].trim().to_lowercase();
                let val = line[pos + 1..]
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim()
                    .to_string();
                match key.as_str() {
                    "aws_access_key_id" => access_key_id = Some(val),
                    "aws_secret_access_key" => secret_access_key = Some(val),
                    "aws_session_token" => session_token = Some(val),
                    _ => {}
                }
            }
        }
    }

    if let (Some(key), Some(secret)) = (access_key_id, secret_access_key) {
        Ok(AwsCredentials {
            access_key_id: key,
            secret_access_key: secret,
            session_token,
        })
    } else {
        Err(StorageError::Unauthorized(format!(
            "AWS credentials for profile '{}' not found in ~/.aws/credentials",
            target_profile
        )))
    }
}

/// Percent-encode a URI path per the SigV4 canonical-request rules: every
/// byte is encoded except unreserved characters (`A-Za-z0-9-_.~`) and the
/// path separator `/`, which must not be re-encoded. Applied to both the
/// signed canonical request and the actual request URL, so a `prefix`
/// containing spaces or other special characters signs correctly instead of
/// producing a misleading 403.
fn uri_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

/// Extract the `<Code>` element from an S3 XML error body, if present.
///
/// S3 returns 403 for both an outright access-denied response and a
/// signature mismatch (a client-side signing bug); the XML `Code` element
/// is the only way to tell them apart, so surface it instead of a single
/// generic "Access Denied" message that conflates the two.
fn s3_error_code(body: &str) -> Option<&str> {
    let start = body.find("<Code>")? + "<Code>".len();
    let end = body[start..].find("</Code>")? + start;
    Some(&body[start..end])
}

fn unauthorized_error(resp: &minreq::Response) -> StorageError {
    let body = resp.as_str().unwrap_or("");
    match s3_error_code(body) {
        Some(code) => StorageError::Unauthorized(format!("S3 request denied: {code}")),
        None => StorageError::Unauthorized(
            "S3 request denied (403); check credentials and request signing".to_string(),
        ),
    }
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut padded_key = [0u8; 64];
    if key.len() > 64 {
        let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
        sha2::Digest::update(&mut hasher, key);
        let hash = sha2::Digest::finalize(hasher);
        padded_key[..32].copy_from_slice(&hash);
    } else {
        padded_key[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= padded_key[i];
        opad[i] ^= padded_key[i];
    }

    let mut inner_hasher = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(&mut inner_hasher, ipad);
    sha2::Digest::update(&mut inner_hasher, msg);
    let inner_hash = sha2::Digest::finalize(inner_hasher);

    let mut outer_hasher = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(&mut outer_hasher, opad);
    sha2::Digest::update(&mut outer_hasher, inner_hash);
    let outer_hash = sha2::Digest::finalize(outer_hasher);

    let mut result = [0u8; 32];
    result.copy_from_slice(&outer_hash);
    result
}

/// The pieces of an S3 request that vary per call; grouped so `sign_request`
/// takes one argument for them instead of five.
struct RequestToSign<'a> {
    method: &'a str,
    url_path: &'a str,
    query_str: &'a str,
    headers: &'a [(&'a str, &'a str)],
    body_sha256: &'a str,
}

/// The date/region context a signature is scoped to.
struct SignatureScope<'a> {
    region: &'a str,
    date_str: &'a str,
    date_only: &'a str,
}

fn sign_request(request: &RequestToSign, creds: &AwsCredentials, scope: &SignatureScope) -> String {
    let mut sorted_headers = request.headers.to_vec();
    sorted_headers.sort_by_key(|a| a.0.to_lowercase());

    let mut canonical_headers = String::new();
    let mut signed_headers = String::new();
    for (name, val) in sorted_headers {
        let name_lower = name.to_lowercase();
        canonical_headers.push_str(&format!("{name_lower}:{val}\n"));
        if !signed_headers.is_empty() {
            signed_headers.push(';');
        }
        signed_headers.push_str(&name_lower);
    }

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        request.method,
        request.url_path,
        request.query_str,
        canonical_headers,
        signed_headers,
        request.body_sha256
    );

    let mut request_hasher = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(&mut request_hasher, canonical_request.as_bytes());
    let canonical_request_hash = hex::encode(sha2::Digest::finalize(request_hasher));

    let credential_scope = format!("{}/{}/s3/aws4_request", scope.date_only, scope.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        scope.date_str, credential_scope, canonical_request_hash
    );

    let k_date = hmac_sha256(
        format!("AWS4{}", creds.secret_access_key).as_bytes(),
        scope.date_only.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, scope.region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        creds.access_key_id, credential_scope, signed_headers, signature
    )
}

pub struct S3BlobStore {
    bucket: String,
    region: String,
    key: String,
    endpoint: String,
    use_path_style: bool,
    profile: Option<String>,
}

impl S3BlobStore {
    pub fn new(
        bucket: String,
        region: String,
        prefix: String,
        endpoint: Option<String>,
        use_path_style: bool,
        profile: Option<String>,
    ) -> Self {
        let key = if prefix.is_empty() {
            "logbook.json".to_string()
        } else if prefix.ends_with('/') {
            format!("{}logbook.json", prefix)
        } else {
            format!("{}/logbook.json", prefix)
        };

        let endpoint = endpoint.unwrap_or_else(|| "s3.amazonaws.com".to_string());

        Self {
            bucket,
            region,
            key,
            endpoint,
            use_path_style,
            profile,
        }
    }

    fn resolve_url_and_host(&self) -> (String, String, String) {
        let mut ep_host = self.endpoint.clone();
        let mut scheme = "https".to_string();

        if ep_host.starts_with("http://") {
            scheme = "http".to_string();
            ep_host = ep_host["http://".len()..].to_string();
        } else if ep_host.starts_with("https://") {
            scheme = "https".to_string();
            ep_host = ep_host["https://".len()..].to_string();
        }

        if self.use_path_style {
            let host = ep_host;
            let path = uri_encode_path(&format!("/{}/{}", self.bucket, self.key));
            let url = format!("{}://{}{}", scheme, host, path);
            (url, path, host)
        } else {
            let host = format!("{}.{}", self.bucket, ep_host);
            let path = uri_encode_path(&format!("/{}", self.key));
            let url = format!("{}://{}{}", scheme, host, path);
            (url, path, host)
        }
    }
}

impl BlobStore for S3BlobStore {
    fn get(&self) -> Result<Option<(Vec<u8>, String)>, StorageError> {
        let creds = load_aws_credentials(self.profile.as_deref())?;
        let (url, path, host) = self.resolve_url_and_host();

        let now = Utc::now();
        let date_str = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_only = now.format("%Y%m%d").to_string();

        let mut headers = vec![
            ("Host", host.as_str()),
            ("X-Amz-Date", date_str.as_str()),
            (
                "X-Amz-Content-Sha256",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ), // SHA256 of empty body
        ];

        if let Some(token) = &creds.session_token {
            headers.push(("X-Amz-Security-Token", token.as_str()));
        }

        let auth = sign_request(
            &RequestToSign {
                method: "GET",
                url_path: &path,
                query_str: "",
                headers: &headers,
                body_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            },
            &creds,
            &SignatureScope {
                region: &self.region,
                date_str: &date_str,
                date_only: &date_only,
            },
        );

        let mut req = minreq::get(&url).with_timeout(10); // 10 seconds timeout

        for (name, val) in headers {
            req = req.with_header(name, val);
        }
        req = req.with_header("Authorization", auth);

        let resp = match req.send() {
            Ok(r) => r,
            Err(e) => return Err(StorageError::Unavailable(format!("S3 request failed: {e}"))),
        };

        match resp.status_code {
            200 => {
                let bytes = resp.as_bytes().to_vec();
                let etag = resp
                    .header("etag")
                    .map(|e| e.trim().to_string())
                    .ok_or_else(|| {
                        StorageError::Corrupt("S3 response missing ETag header".to_string())
                    })?;
                Ok(Some((bytes, etag)))
            }
            404 => Ok(None),
            403 => Err(unauthorized_error(&resp)),
            code => Err(StorageError::Unavailable(format!(
                "S3 server returned unexpected code {code}"
            ))),
        }
    }

    fn put(&self, bytes: &[u8], cond: Precondition) -> Result<String, StorageError> {
        let creds = load_aws_credentials(self.profile.as_deref())?;
        let (url, path, host) = self.resolve_url_and_host();

        let now = Utc::now();
        let date_str = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_only = now.format("%Y%m%d").to_string();

        let mut body_hasher = <sha2::Sha256 as sha2::Digest>::new();
        sha2::Digest::update(&mut body_hasher, bytes);
        let body_sha256 = hex::encode(sha2::Digest::finalize(body_hasher));

        let mut headers = vec![
            ("Host", host.as_str()),
            ("X-Amz-Date", date_str.as_str()),
            ("X-Amz-Content-Sha256", body_sha256.as_str()),
            ("Content-Type", "application/octet-stream"),
        ];

        if let Some(token) = &creds.session_token {
            headers.push(("X-Amz-Security-Token", token.as_str()));
        }

        match &cond {
            Precondition::IfAbsent => {
                headers.push(("If-None-Match", "*"));
            }
            Precondition::IfMatch(etag) => {
                headers.push(("If-Match", etag.as_str()));
            }
        }

        let auth = sign_request(
            &RequestToSign {
                method: "PUT",
                url_path: &path,
                query_str: "",
                headers: &headers,
                body_sha256: &body_sha256,
            },
            &creds,
            &SignatureScope {
                region: &self.region,
                date_str: &date_str,
                date_only: &date_only,
            },
        );

        let mut req = minreq::put(&url).with_timeout(10).with_body(bytes);

        for (name, val) in headers {
            req = req.with_header(name, val);
        }
        req = req.with_header("Authorization", auth);

        let resp = match req.send() {
            Ok(r) => r,
            Err(e) => return Err(StorageError::Unavailable(format!("S3 request failed: {e}"))),
        };

        match resp.status_code {
            200 | 201 | 204 => {
                let etag = resp
                    .header("etag")
                    .map(|e| e.trim().to_string())
                    .unwrap_or_else(|| {
                        // If ETag is missing on PUT, synthesize one
                        let mut h = <sha2::Sha256 as sha2::Digest>::new();
                        sha2::Digest::update(&mut h, bytes);
                        format!("\"{}\"", hex::encode(sha2::Digest::finalize(h)))
                    });
                Ok(etag)
            }
            412 => {
                let actual = resp
                    .header("etag")
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                let expected = match cond {
                    Precondition::IfAbsent => "None".to_string(),
                    Precondition::IfMatch(e) => e,
                };
                Err(StorageError::Conflict { expected, actual })
            }
            403 => Err(unauthorized_error(&resp)),
            code => Err(StorageError::Unavailable(format!(
                "S3 server returned unexpected code {code}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds() -> AwsCredentials {
        AwsCredentials {
            access_key_id: "AKIDEXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: None,
        }
    }

    #[test]
    fn uri_encode_path_preserves_slashes_and_unreserved_chars() {
        assert_eq!(uri_encode_path("/bucket/key"), "/bucket/key");
        assert_eq!(uri_encode_path("/bucket/a-b_c.d~e"), "/bucket/a-b_c.d~e");
    }

    #[test]
    fn uri_encode_path_escapes_special_characters() {
        // A prefix containing a space and a plus must be percent-encoded so
        // the signed path matches the actual request path exactly.
        assert_eq!(uri_encode_path("/bucket/a b+c"), "/bucket/a%20b%2Bc");
        assert_eq!(
            uri_encode_path("/bucket/日本語"),
            "/bucket/%E6%97%A5%E6%9C%AC%E8%AA%9E"
        );
    }

    #[test]
    fn resolve_url_and_host_path_style() {
        let store = S3BlobStore::new(
            "my-bucket".to_string(),
            "us-east-1".to_string(),
            "a b/prefix".to_string(),
            Some("http://localhost:9000".to_string()),
            true,
            None,
        );
        let (url, path, host) = store.resolve_url_and_host();
        assert_eq!(host, "localhost:9000");
        assert_eq!(path, "/my-bucket/a%20b/prefix/logbook.json");
        assert_eq!(
            url,
            "http://localhost:9000/my-bucket/a%20b/prefix/logbook.json"
        );
    }

    #[test]
    fn resolve_url_and_host_virtual_host_style() {
        let store = S3BlobStore::new(
            "my-bucket".to_string(),
            "us-east-1".to_string(),
            "".to_string(),
            None,
            false,
            None,
        );
        let (url, path, host) = store.resolve_url_and_host();
        assert_eq!(host, "my-bucket.s3.amazonaws.com");
        assert_eq!(path, "/logbook.json");
        assert_eq!(url, "https://my-bucket.s3.amazonaws.com/logbook.json");
    }

    #[test]
    fn sign_request_is_deterministic_and_sensitive_to_its_inputs() {
        let headers = [
            ("Host", "my-bucket.s3.amazonaws.com"),
            ("X-Amz-Date", "20260913T000000Z"),
        ];
        let scope = SignatureScope {
            region: "us-east-1",
            date_str: "20260913T000000Z",
            date_only: "20260913",
        };
        let empty_body_sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

        let request = RequestToSign {
            method: "GET",
            url_path: "/logbook.json",
            query_str: "",
            headers: &headers,
            body_sha256: empty_body_sha256,
        };

        let auth_a = sign_request(&request, &creds(), &scope);
        let auth_b = sign_request(&request, &creds(), &scope);
        assert_eq!(auth_a, auth_b, "signing the same request twice must match");
        assert!(auth_a.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20260913/us-east-1/s3/aws4_request"
        ));

        // Changing the body hash (a different payload) must change the
        // signature — otherwise a tampered body would go undetected.
        let mut other_body = "0".repeat(64);
        other_body.push_str(""); // keep length 64
        let request_different_body = RequestToSign {
            body_sha256: &other_body,
            ..request
        };
        let auth_different_body = sign_request(&request_different_body, &creds(), &scope);
        assert_ne!(auth_a, auth_different_body);
    }
}
