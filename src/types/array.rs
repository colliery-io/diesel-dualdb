//! Portable [`Array`] domain newtype — a `Vec<T>` on a column typed
//! [`crate::sql_types::Array<ST>`].
//!
//! PostgreSQL uses a native array of the element type (delegating to diesel's
//! `Array<ST>` ↔ `Vec<T>` support). SQLite has no array type, so the elements
//! are stored as a JSON array in `TEXT` (reusing `serde_json`). Element ordering
//! is preserved; SQLite-side array operators aren't available (it's a JSON value
//! there).
//!
//! A newtype (rather than bare `Vec<T>`) is required: diesel has a blanket
//! `AsExpression` impl that a foreign `Vec` can't coexist with, but a local
//! newtype can.

use diesel::backend::Backend;
use diesel::deserialize::{self, FromSql, FromStaticSqlRow, Queryable};
use diesel::expression::AsExpression;
use diesel::internal::derives::as_expression::Bound;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::{Array as DieselArray, HasSqlType, Nullable, SingleValue, Text};
use diesel::sqlite::{Sqlite, SqliteValue};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::{sql_types, MultiBackend};

/// A portable array. Wraps `Vec<T>`; use it on a column typed
/// [`crate::sql_types::Array<ST>`] where `ST` is the element's SQL type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Array<T>(pub Vec<T>);

impl<T> From<Vec<T>> for Array<T> {
    fn from(value: Vec<T>) -> Self {
        Array(value)
    }
}

impl<T> From<Array<T>> for Vec<T> {
    fn from(value: Array<T>) -> Self {
        value.0
    }
}

// ===== HasSqlType (the marker, per backend) =====

impl<ST: 'static> HasSqlType<sql_types::Array<ST>> for Pg
where
    Pg: HasSqlType<DieselArray<ST>>,
{
    fn metadata(lookup: &mut Self::MetadataLookup) -> Self::TypeMetadata {
        <Pg as HasSqlType<DieselArray<ST>>>::metadata(lookup)
    }
}

impl<ST: 'static> HasSqlType<sql_types::Array<ST>> for Sqlite {
    fn metadata(lookup: &mut Self::MetadataLookup) -> Self::TypeMetadata {
        <Sqlite as HasSqlType<Text>>::metadata(lookup)
    }
}

impl<ST: 'static> HasSqlType<sql_types::Array<ST>> for MultiBackend
where
    Pg: HasSqlType<DieselArray<ST>>,
{
    fn metadata(lookup: &mut Self::MetadataLookup) -> Self::TypeMetadata {
        Self::lookup_sql_type::<sql_types::Array<ST>>(lookup)
    }
}

// ===== PostgreSQL: native array (delegate to diesel's Vec impls) =====

impl<T, ST: 'static> ToSql<sql_types::Array<ST>, Pg> for Array<T>
where
    T: std::fmt::Debug, // ToSql has a Debug supertrait
    Vec<T>: ToSql<DieselArray<ST>, Pg>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        <Vec<T> as ToSql<DieselArray<ST>, Pg>>::to_sql(&self.0, out)
    }
}

impl<T, ST: 'static> FromSql<sql_types::Array<ST>, Pg> for Array<T>
where
    Vec<T>: FromSql<DieselArray<ST>, Pg>,
{
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        <Vec<T> as FromSql<DieselArray<ST>, Pg>>::from_sql(bytes).map(Array)
    }
}

// ===== SQLite: JSON array in TEXT =====

impl<T, ST: 'static> ToSql<sql_types::Array<ST>, Sqlite> for Array<T>
where
    T: Serialize + std::fmt::Debug, // ToSql has a Debug supertrait
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(serde_json::to_string(&self.0)?);
        Ok(IsNull::No)
    }
}

impl<T, ST: 'static> FromSql<sql_types::Array<ST>, Sqlite> for Array<T>
where
    T: DeserializeOwned,
{
    fn from_sql(bytes: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        let s = <String as FromSql<Text, Sqlite>>::from_sql(bytes)?;
        Ok(Array(serde_json::from_str(&s)?))
    }
}

// ===== MultiBackend bridge =====

impl<T, ST> ToSql<sql_types::Array<ST>, MultiBackend> for Array<T>
where
    ST: Send + 'static,
    // Send + Sync + 'static: the bind collector boxes the value across the enum.
    Array<T>: ToSql<sql_types::Array<ST>, Pg>
        + ToSql<sql_types::Array<ST>, Sqlite>
        + Send
        + Sync
        + 'static,
    Pg: HasSqlType<sql_types::Array<ST>>,
    Sqlite: HasSqlType<sql_types::Array<ST>>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, MultiBackend>) -> serialize::Result {
        out.set_value((sql_types::Array::<ST>::default(), self));
        Ok(IsNull::No)
    }
}

impl<T, ST> FromSql<sql_types::Array<ST>, MultiBackend> for Array<T>
where
    ST: 'static,
    Array<T>: FromSql<sql_types::Array<ST>, Pg> + FromSql<sql_types::Array<ST>, Sqlite>,
{
    fn from_sql(bytes: <MultiBackend as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        bytes.from_sql::<Self, sql_types::Array<ST>>()
    }
}

// ===== Expression / Queryable wiring =====

impl<T, ST> AsExpression<sql_types::Array<ST>> for Array<T>
where
    sql_types::Array<ST>: SingleValue,
{
    type Expression = Bound<sql_types::Array<ST>, Self>;
    fn as_expression(self) -> Self::Expression {
        Bound::new(self)
    }
}

impl<T, ST> AsExpression<Nullable<sql_types::Array<ST>>> for Array<T>
where
    sql_types::Array<ST>: SingleValue,
{
    type Expression = Bound<Nullable<sql_types::Array<ST>>, Self>;
    fn as_expression(self) -> Self::Expression {
        Bound::new(self)
    }
}

impl<T, ST> AsExpression<sql_types::Array<ST>> for &Array<T>
where
    sql_types::Array<ST>: SingleValue,
{
    type Expression = Bound<sql_types::Array<ST>, Self>;
    fn as_expression(self) -> Self::Expression {
        Bound::new(self)
    }
}

impl<T, ST, DB> Queryable<sql_types::Array<ST>, DB> for Array<T>
where
    DB: Backend,
    ST: SingleValue,
    Array<T>: FromStaticSqlRow<sql_types::Array<ST>, DB>,
{
    type Row = Self;
    fn build(row: Self::Row) -> deserialize::Result<Self> {
        Ok(row)
    }
}
