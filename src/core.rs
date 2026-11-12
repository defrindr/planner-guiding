//! Backwards-compatible facade for callers that still import `crate::core`.
//!
//! The implementation now lives in the application layer. Keeping this
//! re-export avoids changing the CLI and any downstream integration code in
//! the same migration step.
pub use crate::application::service::ShopCore;
