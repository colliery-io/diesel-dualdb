//! The `MultiBackend` bridge — the milestone that makes write-once real.
//!
//! For each portable SQL type we implement three traits against the
//! `MultiBackend` that `#[derive(MultiConnection)]` generates for
//! [`crate::DualConnection`]:
//!
//! - [`HasSqlType`](diesel::sql_types::HasSqlType) — so `QueryMetadata` is
//!   satisfied and the query builder knows the type exists on the unified
//!   backend (this is the hole #4079 left open for non-core types like `Uuid`);
//! - [`ToSql`](diesel::serialize::ToSql) `<_, MultiBackend>` — dispatches
//!   binding to the active inner backend;
//! - [`FromSql`](diesel::deserialize::FromSql) `<_, MultiBackend>` — dispatches
//!   deserialization off the active inner backend's raw value.
//!
//! These mirror, exactly, the impls the derive generates for the core required
//! types (Integer, Text, Bool, …). The derive exposes the two helpers that do
//! the actual per-arm dispatch — `MultiBackend::lookup_sql_type` and
//! `MultiRawValue::from_sql` — so each impl is a one-liner over them. Because
//! `MultiBackend` is defined in this crate, these impls are local: no orphan
//! rule problem.
//!
//! Non-generic types use the [`bridge!`](crate::bridge) macro, which emits these
//! three impls from `bridge!(Marker, Newtype)`. The generic `Json<T>` is
//! hand-written (its bounds don't fit the macro). Each type's bridge lives
//! behind the same feature gate as the type itself.

// ----- Uuid -----

#[cfg(feature = "uuid")]
crate::bridge!(crate::sql_types::Uuid, crate::types::Uuid);

// ----- Bytes -----

crate::bridge!(crate::sql_types::Bytes, crate::types::Bytes);

// ----- Timestamp -----

#[cfg(feature = "chrono")]
crate::bridge!(crate::sql_types::Timestamp, crate::types::Timestamp);

// ----- Decimal -----

#[cfg(feature = "decimal")]
crate::bridge!(crate::sql_types::Decimal, crate::types::Decimal);

// ----- Json<T> -----
//
// Hand-written, not via `bridge!`: `Json<T>` is generic and its `MultiBackend`
// `ToSql` needs `T: Send + Sync + 'static` (the bind collector boxes the value
// across the backend enum) while `FromSql` needs only `DeserializeOwned`. Those
// split, impl-specific bounds don't fit the `bridge!(Marker, Newtype)` shape.

#[cfg(feature = "serde_json")]
mod json {
    use diesel::backend::Backend;
    use diesel::deserialize::{self, FromSql};
    use diesel::serialize::{self, IsNull, Output, ToSql};
    use diesel::sql_types::HasSqlType;
    use serde::de::DeserializeOwned;
    use serde::Serialize;

    use crate::{sql_types, types, MultiBackend};

    impl HasSqlType<sql_types::Json> for MultiBackend {
        fn metadata(lookup: &mut Self::MetadataLookup) -> Self::TypeMetadata {
            Self::lookup_sql_type::<sql_types::Json>(lookup)
        }
    }

    impl<T> ToSql<sql_types::Json, MultiBackend> for types::Json<T>
    where
        // Send + Sync: the MultiBackend bind collector boxes the value across the
        // backend enum, so it must be thread-safe (the concrete Pg/Sqlite impls
        // don't require this).
        T: Serialize + std::fmt::Debug + Send + Sync + 'static,
    {
        fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, MultiBackend>) -> serialize::Result {
            out.set_value((sql_types::Json, self));
            Ok(IsNull::No)
        }
    }

    impl<T> FromSql<sql_types::Json, MultiBackend> for types::Json<T>
    where
        T: DeserializeOwned,
    {
        fn from_sql(bytes: <MultiBackend as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
            bytes.from_sql::<Self, sql_types::Json>()
        }
    }
}
