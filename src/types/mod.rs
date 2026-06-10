//! Portable domain newtypes.
//!
//! Each type wraps a Rust value and carries `ToSql`/`FromSql` impls against
//! `Pg` and `Sqlite` (the concrete backends). The bridge onto the generated
//! `MultiBackend` — what makes one-arm `get_result`/`RETURNING` work — lives in
//! [`crate::backend`] and is added in DDB-T-0003.

#[cfg(feature = "array")]
mod array;
mod bytes;
#[cfg(feature = "decimal")]
mod decimal;
#[cfg(feature = "serde_json")]
mod json;
#[cfg(feature = "chrono")]
mod timestamp;
#[cfg(feature = "uuid")]
mod uuid;

#[cfg(feature = "array")]
pub use array::Array;
pub use bytes::Bytes;
#[cfg(feature = "decimal")]
pub use decimal::Decimal;
#[cfg(feature = "serde_json")]
pub use json::Json;
#[cfg(feature = "chrono")]
pub use timestamp::Timestamp;
#[cfg(feature = "uuid")]
pub use uuid::Uuid;
