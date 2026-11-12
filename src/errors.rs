use axum::http::StatusCode;

#[derive(Debug, Clone)]
pub struct AppError {
    pub status: StatusCode,
    pub message: String,
}

impl AppError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

pub fn error_body(message: &str) -> serde_json::Value {
    serde_json::json!({"error": message})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_serializes_to_legacy_error_shape() {
        let value = error_body("not found");
        assert_eq!(value, serde_json::json!({"error": "not found"}));
    }
}
