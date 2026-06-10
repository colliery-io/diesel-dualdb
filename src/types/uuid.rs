//! Portable [`Uuid`] domain newtype.
//!
//! Postgres delegates to diesel's native `uuid` support; SQLite stores the raw
//! 16 bytes as a `BLOB`. The marker SQL type is [`crate::sql_types::Uuid`].

use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::Binary;
use diesel::sqlite::{Sqlite, SqliteValue};

use crate::sql_types;

/// A UUID that serializes portably across PostgreSQL and SQLite.
///
/// Wraps [`uuid::Uuid`]. Use it on a column typed [`crate::sql_types::Uuid`].
#[derive(AsExpression, FromSqlRow, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[diesel(sql_type = crate::sql_types::Uuid)]
pub struct Uuid(pub uuid::Uuid);

impl From<uuid::Uuid> for Uuid {
    fn from(value: uuid::Uuid) -> Self {
        Uuid(value)
    }
}

impl From<Uuid> for uuid::Uuid {
    fn from(value: Uuid) -> Self {
        value.0
    }
}

// --- PostgreSQL: delegate to diesel's native uuid support ---

impl ToSql<sql_types::Uuid, Pg> for Uuid {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <uuid::Uuid as ToSql<diesel::sql_types::Uuid, Pg>>::to_sql(&self.0, out)
    }
}

impl FromSql<sql_types::Uuid, Pg> for Uuid {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        <uuid::Uuid as FromSql<diesel::sql_types::Uuid, Pg>>::from_sql(bytes).map(Uuid)
    }
}

// --- SQLite: 16-byte BLOB ---

impl ToSql<sql_types::Uuid, Sqlite> for Uuid {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0.as_bytes().to_vec());
        Ok(IsNull::No)
    }
}

impl FromSql<sql_types::Uuid, Sqlite> for Uuid {
    fn from_sql(bytes: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let blob = <Vec<u8> as FromSql<Binary, Sqlite>>::from_sql(bytes)?;
        let inner =
            uuid::Uuid::from_slice(&blob).map_err(|e| format!("invalid UUID bytes: {e}"))?;
        Ok(Uuid(inner))
    }
}
