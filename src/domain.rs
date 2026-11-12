use std::collections::HashMap;

use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha2::Sha256;

pub type HmacSha256 = Hmac<Sha256>;

pub const DEFAULT_ORDER_NO: i64 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Paid,
    Shipped,
    Cancelled,
    Refunded,
}

impl OrderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Paid => "paid",
            Self::Shipped => "shipped",
            Self::Cancelled => "cancelled",
            Self::Refunded => "refunded",
        }
    }
}

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

    #[test]
    fn legacy_password_and_total_are_stable() {
        assert_eq!(
            hash_password("admin123"),
            "0192023a7bbd73250516f069df18b500"
        );
        assert_eq!(calculate_total(100.0, 10), 90.0);
    }

    #[test]
    fn webhook_signature_accepts_hex_hmac_only() {
        let body = br#"{"status":"settled"}"#;
        let mut mac = HmacSha256::new_from_slice(b"secret").unwrap();
        mac.update(body);
        let signature = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert!(verify_webhook_signature("secret", body, &signature));
        assert!(!verify_webhook_signature("secret", body, "not-hex"));
    }
}
