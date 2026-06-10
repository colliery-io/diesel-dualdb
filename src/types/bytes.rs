//! Portable [`Bytes`] domain newtype.
//!
//! Both backends delegate to diesel's native `Binary` support: PostgreSQL
//! `bytea`, SQLite `BLOB`. No optional dependency — `std` only.

use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Binary;
use diesel::sqlite::{Sqlite, SqliteValue};

use crate::sql_types;

/// A binary blob that serializes portably across PostgreSQL and SQLite.
///
/// Wraps `Vec<u8>`. Use it on a column typed [`crate::sql_types::Bytes`].
#[derive(AsExpression, FromSqlRow, Clone, Debug, PartialEq, Eq, Hash, Default)]
#[diesel(sql_type = crate::sql_types::Bytes)]
pub struct Bytes(pub Vec<u8>);

impl From<Vec<u8>> for Bytes {
    fn from(value: Vec<u8>) -> Self {
        Bytes(value)
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(value: Bytes) -> Self {
        value.0
    }
}

// --- PostgreSQL: native bytea ---

impl ToSql<sql_types::Bytes, Pg> for Bytes {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <Vec<u8> as ToSql<Binary, Pg>>::to_sql(&self.0, out)
    }
}

impl FromSql<sql_types::Bytes, Pg> for Bytes {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        <Vec<u8> as FromSql<Binary, Pg>>::from_sql(bytes).map(Bytes)
    }
}

// --- SQLite: BLOB ---

impl ToSql<sql_types::Bytes, Sqlite> for Bytes {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        <Vec<u8> as ToSql<Binary, Sqlite>>::to_sql(&self.0, out)
    }
}

impl FromSql<sql_types::Bytes, Sqlite> for Bytes {
    fn from_sql(bytes: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        <Vec<u8> as FromSql<Binary, Sqlite>>::from_sql(bytes).map(Bytes)
    }
}
