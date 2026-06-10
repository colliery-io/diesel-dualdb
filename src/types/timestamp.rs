//! Portable [`Timestamp`] domain newtype.
//!
//! PostgreSQL uses native `timestamptz` (delegating to diesel's chrono
//! support). SQLite stores a **fixed-format** RFC3339 string per ADR
//! DDB-A-0001: UTC, `Z` suffix, exactly 6 fractional digits — chosen so that
//! string comparison equals chronological comparison (`ORDER BY` / range
//! queries work without special-casing). Both backends carry microsecond
//! precision; finer precision is truncated.

use chrono::{DateTime, Utc};
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::{Text, Timestamptz};
use diesel::sqlite::{Sqlite, SqliteValue};

use crate::sql_types;

/// Canonical SQLite text format (ADR DDB-A-0001). `%.6f` always emits exactly
/// six fractional digits, keeping every stored value the same width so
/// lexicographic order matches chronological order.
const SQLITE_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.6fZ";

/// A UTC timestamp that serializes portably across PostgreSQL and SQLite.
///
/// Wraps [`chrono::DateTime<Utc>`]. Use it on a column typed
/// [`crate::sql_types::Timestamp`].
#[derive(AsExpression, FromSqlRow, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[diesel(sql_type = crate::sql_types::Timestamp)]
pub struct Timestamp(pub DateTime<Utc>);

impl From<DateTime<Utc>> for Timestamp {
    fn from(value: DateTime<Utc>) -> Self {
        Timestamp(value)
    }
}

impl From<Timestamp> for DateTime<Utc> {
    fn from(value: Timestamp) -> Self {
        value.0
    }
}

// --- PostgreSQL: native timestamptz ---

impl ToSql<sql_types::Timestamp, Pg> for Timestamp {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <DateTime<Utc> as ToSql<Timestamptz, Pg>>::to_sql(&self.0, out)
    }
}

impl FromSql<sql_types::Timestamp, Pg> for Timestamp {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        <DateTime<Utc> as FromSql<Timestamptz, Pg>>::from_sql(bytes).map(Timestamp)
    }
}

// --- SQLite: fixed-format RFC3339 TEXT ---

impl ToSql<sql_types::Timestamp, Sqlite> for Timestamp {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0.format(SQLITE_FORMAT).to_string());
        Ok(IsNull::No)
    }
}

impl FromSql<sql_types::Timestamp, Sqlite> for Timestamp {
    fn from_sql(bytes: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let s = <String as FromSql<Text, Sqlite>>::from_sql(bytes)?;
        let dt = DateTime::parse_from_rfc3339(&s)
            .map_err(|e| format!("invalid RFC3339 timestamp {s:?}: {e}"))?
            .with_timezone(&Utc);
        Ok(Timestamp(dt))
    }
}
