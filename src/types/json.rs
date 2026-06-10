//! Portable [`Json`] domain newtype.
//!
//! Wraps any `serde`-serializable value. PostgreSQL stores it as native
//! `jsonb` (the `0x01` version byte + UTF-8 JSON wire format); SQLite stores
//! the serialized JSON as `TEXT`. SQLite 3.45's native JSONB is deferred — we
//! keep TEXT for portability and debuggability.

use std::io::Write;

use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::Text;
use diesel::sqlite::{Sqlite, SqliteValue};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::sql_types;

/// A JSON value that serializes portably across PostgreSQL and SQLite.
///
/// Wraps any `T: Serialize + DeserializeOwned` (e.g. a typed struct, or
/// `serde_json::Value`). Use it on a column typed [`crate::sql_types::Json`].
#[derive(AsExpression, FromSqlRow, Clone, Debug, PartialEq)]
#[diesel(sql_type = crate::sql_types::Json)]
pub struct Json<T>(pub T);

impl<T> From<T> for Json<T> {
    fn from(value: T) -> Self {
        Json(value)
    }
}

// --- PostgreSQL: native jsonb (0x01 version byte + UTF-8 JSON) ---

impl<T> ToSql<sql_types::Json, Pg> for Json<T>
where
    T: Serialize + std::fmt::Debug,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        out.write_all(&[1])?; // jsonb format version 1
        serde_json::to_writer(out, &self.0)?;
        Ok(IsNull::No)
    }
}

impl<T> FromSql<sql_types::Json, Pg> for Json<T>
where
    T: DeserializeOwned,
{
    fn from_sql(value: PgValue<'_>) -> deserialize::Result<Self> {
        let bytes = value.as_bytes();
        match bytes.first() {
            Some(1) => Ok(Json(serde_json::from_slice(&bytes[1..])?)),
            _ => Err("unsupported jsonb encoding (expected version 1)".into()),
        }
    }
}

// --- SQLite: serialized JSON TEXT ---

impl<T> ToSql<sql_types::Json, Sqlite> for Json<T>
where
    T: Serialize + std::fmt::Debug,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(serde_json::to_string(&self.0)?);
        Ok(IsNull::No)
    }
}

impl<T> FromSql<sql_types::Json, Sqlite> for Json<T>
where
    T: DeserializeOwned,
{
    fn from_sql(bytes: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let s = <String as FromSql<Text, Sqlite>>::from_sql(bytes)?;
        Ok(Json(serde_json::from_str(&s)?))
    }
}
