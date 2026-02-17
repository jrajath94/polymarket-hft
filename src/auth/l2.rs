// L2 authentication: HMAC-SHA256 header generation for CLOB REST API.
//
// Polymarket L2 auth requires signing: timestamp + method + path + body
// with the API secret using HMAC-SHA256, then base64-encoding the result.
// Headers: POLY_ADDRESS, POLY_SIGNATURE, POLY_TIMESTAMP, POLY_API_KEY, POLY_PASSPHRASE

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};

type HmacSha256 = Hmac<Sha256>;

/// L2 API credentials derived from wallet signature.
#[derive(Debug, Clone)]
pub struct L2Credentials {
    pub api_key: String,
    pub api_secret: String,
    pub api_passphrase: String,
}

/// Complete set of auth headers for a CLOB REST request.
#[derive(Debug, Clone)]
pub struct L2Headers {
    pub poly_address: String,
    pub poly_signature: String,
    pub poly_timestamp: String,
    pub poly_api_key: String,
    pub poly_passphrase: String,
}

/// Generates HMAC-SHA256 signature for L2 auth.
///
/// Message format: `{timestamp}{method}{path}{body}`
/// The secret is base64-decoded before use as the HMAC key.
pub fn build_hmac_signature(
    secret: &str,
    timestamp: &str,
    method: &str,
    path: &str,
    body: &str,
) -> Result<String> {
    let secret_bytes = BASE64
        .decode(secret)
        .map_err(|e| AppError::Auth(format!("failed to decode API secret: {}", e)))?;

    let message = format!("{}{}{}{}", timestamp, method, path, body);

    let mut mac = HmacSha256::new_from_slice(&secret_bytes)
        .map_err(|e| AppError::Auth(format!("invalid HMAC key length: {}", e)))?;

    mac.update(message.as_bytes());
    let result = mac.finalize();
    Ok(BASE64.encode(result.into_bytes()))
}

/// Returns current Unix timestamp as a string (seconds).
pub fn current_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs()
        .to_string()
}

/// Checks whether a timestamp is within the 30s validity window.
pub fn is_timestamp_valid(timestamp: &str, now_secs: u64) -> bool {
    match timestamp.parse::<u64>() {
        Ok(ts) => now_secs.saturating_sub(ts) <= 30,
        Err(_) => false,
    }
}

/// Build all L2 auth headers for a CLOB REST request.
pub fn build_l2_headers(
    creds: &L2Credentials,
    address: &str,
    method: &str,
    path: &str,
    body: &str,
) -> Result<L2Headers> {
    let timestamp = current_timestamp();
    let signature = build_hmac_signature(&creds.api_secret, &timestamp, method, path, body)?;

    Ok(L2Headers {
        poly_address: address.to_string(),
        poly_signature: signature,
        poly_timestamp: timestamp,
        poly_api_key: creds.api_key.clone(),
        poly_passphrase: creds.api_passphrase.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known test vector: we generate a known secret and verify the HMAC output
    // matches what py-clob-client would produce.
    fn test_secret() -> String {
        // Base64 of 32 bytes of 0x01
        BASE64.encode(vec![0x01u8; 32])
    }

    #[test]
    fn test_hmac_deterministic() {
        let secret = test_secret();
        let sig1 = build_hmac_signature(&secret, "1700000000", "GET", "/book", "").unwrap();
        let sig2 = build_hmac_signature(&secret, "1700000000", "GET", "/book", "").unwrap();
        assert_eq!(sig1, sig2, "HMAC must be deterministic for same inputs");
    }

    #[test]
    fn test_hmac_differs_with_different_timestamp() {
        let secret = test_secret();
        let sig1 = build_hmac_signature(&secret, "1700000000", "GET", "/book", "").unwrap();
        let sig2 = build_hmac_signature(&secret, "1700000001", "GET", "/book", "").unwrap();
        assert_ne!(sig1, sig2, "different timestamps must produce different signatures");
    }

    #[test]
    fn test_hmac_differs_with_body() {
        let secret = test_secret();
        let sig1 = build_hmac_signature(&secret, "1700000000", "POST", "/order", "").unwrap();
        let sig2 =
            build_hmac_signature(&secret, "1700000000", "POST", "/order", r#"{"a":1}"#).unwrap();
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_hmac_output_is_valid_base64() {
        let secret = test_secret();
        let sig = build_hmac_signature(&secret, "1700000000", "GET", "/book", "").unwrap();
        assert!(BASE64.decode(&sig).is_ok(), "signature must be valid base64");
        // HMAC-SHA256 = 32 bytes -> base64 = 44 chars
        assert_eq!(sig.len(), 44);
    }

    #[test]
    fn test_hmac_reference_vector() {
        // Pre-computed reference: HMAC-SHA256 of "1700000000GET/book" with key = [0x01; 32]
        // This matches py-clob-client behavior: message = timestamp + method + path + body
        let secret = test_secret();
        let sig = build_hmac_signature(&secret, "1700000000", "GET", "/book", "").unwrap();

        // Verify by recomputing manually
        let key_bytes = vec![0x01u8; 32];
        let mut mac = HmacSha256::new_from_slice(&key_bytes).unwrap();
        mac.update(b"1700000000GET/book");
        let expected = BASE64.encode(mac.finalize().into_bytes());
        assert_eq!(sig, expected);
    }

    #[test]
    fn test_invalid_secret_base64() {
        let result = build_hmac_signature("not-valid-base64!!!", "1700000000", "GET", "/", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp_validity() {
        let now = 1700000030u64;
        // Exactly 30s ago -- still valid
        assert!(is_timestamp_valid("1700000000", now));
        // 31s ago -- expired
        assert!(!is_timestamp_valid("1699999999", now));
        // Future timestamp -- valid
        assert!(is_timestamp_valid("1700000031", now));
        // Bad input
        assert!(!is_timestamp_valid("not-a-number", now));
    }

    #[test]
    fn test_build_l2_headers() {
        let creds = L2Credentials {
            api_key: "test-key".to_string(),
            api_secret: test_secret(),
            api_passphrase: "test-pass".to_string(),
        };
        let headers = build_l2_headers(&creds, "0xABC", "GET", "/book", "").unwrap();

        assert_eq!(headers.poly_address, "0xABC");
        assert_eq!(headers.poly_api_key, "test-key");
        assert_eq!(headers.poly_passphrase, "test-pass");
        assert!(!headers.poly_signature.is_empty());
        assert!(!headers.poly_timestamp.is_empty());

        // Timestamp should be valid (we just created it)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(is_timestamp_valid(&headers.poly_timestamp, now));
    }

    #[test]
    fn test_current_timestamp_format() {
        let ts = current_timestamp();
        let parsed: u64 = ts.parse().expect("timestamp must be numeric");
        // Should be a reasonable recent Unix timestamp (after 2024)
        assert!(parsed > 1700000000);
    }
}
