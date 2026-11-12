use std::collections::HashMap;

use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha2::Sha256;

pub type HmacSha256 = Hmac<Sha256>;

pub const DEFAULT_ORDER_NO: i64 = 1000;

pub fn hash_password(password: &str) -> String {
    format!("{:x}", Md5::digest(password.as_bytes()))
}

pub fn make_token(username: &str, unix_seconds: i64) -> String {
    let mut hasher = Md5::new();
    hasher.update(format!("{username}:{unix_seconds}").as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn calculate_total(subtotal: f64, percent: i32) -> f64 {
    let total = if percent != 0 {
        subtotal * (1.0 - f64::from(percent) / 100.0)
    } else {
        subtotal
    };
    (total * 100.0).round() / 100.0
}

pub fn line_total(price: f64, quantity: i32) -> f64 {
    price * f64::from(quantity)
}

pub fn verify_webhook_signature(secret: &str, body: &[u8], signature_hex: &str) -> bool {
    let Ok(expected) = hex_decode(signature_hex) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

#[allow(clippy::manual_is_multiple_of)]
fn hex_decode(value: &str) -> Result<Vec<u8>, ()> {
    if value.len() % 2 != 0 {
        return Err(());
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

#[derive(Debug, Default)]
pub struct SessionStore {
    tokens: HashMap<String, i32>,
}

impl SessionStore {
    pub fn insert(&mut self, token: String, user_id: i32) {
        self.tokens.insert(token, user_id);
    }
    pub fn user_id(&self, token: &str) -> Option<i32> {
        self.tokens.get(token).copied()
    }
    pub fn clear(&mut self) {
        self.tokens.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn legacy_password_is_stable_and_input_sensitive() {
        assert_eq!(
            hash_password("admin123"),
            "0192023a7bbd73250516f069df18b500"
        );
        assert_eq!(hash_password(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_ne!(hash_password("admin123"), hash_password("admin124"));
    }

    #[test]
    fn make_token_is_deterministic_and_time_sensitive() {
        let first = make_token("alice", 1_700_000_000);
        assert_eq!(first, make_token("alice", 1_700_000_000));
        assert_ne!(first, make_token("alice", 1_700_000_001));
        assert_ne!(first, make_token("bob", 1_700_000_000));
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn calculate_total_applies_percent_and_rounds_cents() {
        assert_eq!(calculate_total(100.0, 0), 100.0);
        assert_eq!(calculate_total(100.0, 10), 90.0);
        assert_eq!(calculate_total(50.0, 50), 25.0);
        assert_eq!(calculate_total(50.0, 100), 0.0);
        assert_eq!(calculate_total(50.0, 200), -50.0);
        let discounted = calculate_total(89.99 * 2.0, 10);
        assert!((discounted - 161.98).abs() < 1e-9);
    }

    #[test]
    fn line_total_multiplies_price_and_quantity() {
        assert_eq!(line_total(12.5, 3), 37.5);
        assert_eq!(line_total(12.5, 0), 0.0);
        assert_eq!(line_total(10.0, -2), -20.0);
    }

    #[test]
    fn webhook_signature_accepts_valid_and_uppercase_hex() {
        let body = br#"{"status":"settled"}"#;
        let expected = signature("secret", body);
        assert!(verify_webhook_signature("secret", body, &expected));
        assert!(verify_webhook_signature(
            "secret",
            body,
            &expected.to_uppercase()
        ));
    }

    #[test]
    fn webhook_signature_rejects_wrong_secret_or_body() {
        let body = br#"{"status":"settled"}"#;
        let expected = signature("secret", body);
        assert!(!verify_webhook_signature("other", body, &expected));
        assert!(!verify_webhook_signature(
            "secret",
            br#"{"status":"failed"}"#,
            &expected
        ));
    }

    #[test]
    fn webhook_signature_rejects_malformed_or_empty_hex() {
        let body = br#"{"status":"settled"}"#;
        assert!(!verify_webhook_signature("secret", body, "not-hex"));
        assert!(!verify_webhook_signature("secret", body, "0"));
        assert!(!verify_webhook_signature("secret", body, "zz"));
        assert!(!verify_webhook_signature("secret", body, ""));
    }

    #[test]
    fn session_store_inserts_looks_up_and_clears() {
        let mut sessions = SessionStore::default();
        assert_eq!(sessions.user_id("token"), None);
        sessions.insert("token".into(), 7);
        assert_eq!(sessions.user_id("token"), Some(7));
        sessions.insert("token".into(), 9);
        assert_eq!(sessions.user_id("token"), Some(9));
        sessions.clear();
        assert_eq!(sessions.user_id("token"), None);
    }
}
