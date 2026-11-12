//! HTTP boundary for the ShopCore API.
//!
//! This module deliberately contains no persistence logic.  `ShopCore` is the
//! integration seam for the domain/repository agent: the HTTP layer translates
//! requests into a stable, legacy-compatible command and translates the result
//! back into an Axum response.

mod error;
mod models;
mod routes;

pub use error::ApiError;
pub use models::{ApiFuture, ApiRequest, ApiResponse, HttpCore, HttpMethod};
pub use routes::router;
