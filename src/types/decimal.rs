//! Portable [`Decimal`] domain newtype.
//!
//! PostgreSQL uses native `numeric` (delegating to diesel's `bigdecimal`
//! support). SQLite has no decimal type, so the canonical decimal **string** is
//! stored as `TEXT` — an exact round-trip. SQLite-side arithmetic/ordering on
//! that text is not guaranteed; do decimal math in Rust.

use std::str::FromStr;

use bigdecimal::BigDecimal;
use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::{Numeric, Text};
use diesel::sqlite::{Sqlite, SqliteValue};

use crate::sql_types;

/// An arbitrary-precision decimal that serializes portably across PostgreSQL and
/// SQLite. Wraps [`bigdecimal::BigDecimal`]; use it on a column typed
/// [`crate::sql_types::Decimal`].
#[derive(AsExpression, FromSqlRow, Clone, Debug, PartialEq)]
#[diesel(sql_type = crate::sql_types::Decimal)]
pub struct Decimal(pub BigDecimal);

impl From<BigDecimal> for Decimal {
    fn from(value: BigDecimal) -> Self {
        Decimal(value)
    }
}

impl From<Decimal> for BigDecimal {
    fn from(value: Decimal) -> Self {
        value.0
    }
}

// --- PostgreSQL: native numeric ---

impl ToSql<sql_types::Decimal, Pg> for Decimal {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <BigDecimal as ToSql<Numeric, Pg>>::to_sql(&self.0, out)
    }
}

impl FromSql<sql_types::Decimal, Pg> for Decimal {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        <BigDecimal as FromSql<Numeric, Pg>>::from_sql(bytes).map(Decimal)
    }
}

// --- SQLite: canonical decimal string as TEXT ---

impl ToSql<sql_types::Decimal, Sqlite> for Decimal {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0.to_string());
        Ok(IsNull::No)
    }
}

impl FromSql<sql_types::Decimal, Sqlite> for Decimal {
    fn from_sql(bytes: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let s = <String as FromSql<Text, Sqlite>>::from_sql(bytes)?;
        let value = BigDecimal::from_str(&s).map_err(|e| format!("invalid decimal {s:?}: {e}"))?;
        Ok(Decimal(value))
    }
}
