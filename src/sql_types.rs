//! Portable marker SQL types.
//!
//! Each marker names its native representation per backend via the
//! `postgres_type` / `sqlite_type` attributes. The marker carries no data — it
//! exists so columns and the domain newtypes in [`crate::types`] can be typed
//! against a single logical type that resolves correctly on either backend.

/// Portable UUID.
///
/// - PostgreSQL: native `uuid`.
/// - SQLite: 16-byte `BLOB` (chosen over TEXT for index size).
#[cfg(feature = "uuid")]
#[derive(diesel::sql_types::SqlType, diesel::query_builder::QueryId, Clone, Copy, Debug)]
#[diesel(postgres_type(name = "uuid"))]
#[diesel(sqlite_type(name = "Binary"))]
pub struct Uuid;

/// Portable binary blob.
///
/// - PostgreSQL: `bytea`.
/// - SQLite: `BLOB`.
#[derive(diesel::sql_types::SqlType, diesel::query_builder::QueryId, Clone, Copy, Debug)]
#[diesel(postgres_type(name = "bytea"))]
#[diesel(sqlite_type(name = "Binary"))]
pub struct Bytes;

/// Portable UTC timestamp.
///
/// - PostgreSQL: native `timestamptz`.
/// - SQLite: fixed-format RFC3339 `TEXT` (see ADR DDB-A-0001) — UTC, `Z`,
///   6-digit subseconds, so lexicographic order equals chronological order.
#[cfg(feature = "chrono")]
#[derive(diesel::sql_types::SqlType, diesel::query_builder::QueryId, Clone, Copy, Debug)]
#[diesel(postgres_type(name = "timestamptz"))]
#[diesel(sqlite_type(name = "Text"))]
pub struct Timestamp;

// `diesel::table!` recognizes a column typed `Timestamp` as temporal and emits
// `column + interval` / `column - interval` operators, which require the SQL
// type to implement these marker traits (native timestamp types do). Mirror
// that so our marker is a drop-in.
#[cfg(feature = "chrono")]
impl diesel::sql_types::ops::Add for Timestamp {
    type Rhs = diesel::sql_types::Interval;
    type Output = Timestamp;
}

#[cfg(feature = "chrono")]
impl diesel::sql_types::ops::Sub for Timestamp {
    type Rhs = diesel::sql_types::Interval;
    type Output = Timestamp;
}

/// Portable JSON.
///
/// - PostgreSQL: native `jsonb`.
/// - SQLite: `TEXT` (serialized JSON; SQLite 3.45 JSONB is deferred).
#[cfg(feature = "serde_json")]
#[derive(diesel::sql_types::SqlType, diesel::query_builder::QueryId, Clone, Copy, Debug)]
#[diesel(postgres_type(name = "jsonb"))]
#[diesel(sqlite_type(name = "Text"))]
pub struct Json;

/// Portable arbitrary-precision decimal.
///
/// - PostgreSQL: native `numeric`.
/// - SQLite: the canonical decimal string as `TEXT` — exact round-trip. Note
///   SQLite-side arithmetic/ordering on that text is *not* guaranteed; treat it
///   as a value store and do decimal math in Rust (`bigdecimal`).
#[cfg(feature = "decimal")]
#[derive(diesel::sql_types::SqlType, diesel::query_builder::QueryId, Clone, Copy, Debug)]
#[diesel(postgres_type(name = "numeric"))]
#[diesel(sqlite_type(name = "Text"))]
pub struct Decimal;

// `diesel::table!` treats a numeric column as arithmetic and emits
// `+ - * /` operators, which require the SQL type to implement these marker
// traits (diesel's native `Numeric` does). Mirror that so our marker is a
// drop-in in `table!` definitions.
#[cfg(feature = "decimal")]
impl diesel::sql_types::ops::Add for Decimal {
    type Rhs = Decimal;
    type Output = Decimal;
}

#[cfg(feature = "decimal")]
impl diesel::sql_types::ops::Sub for Decimal {
    type Rhs = Decimal;
    type Output = Decimal;
}

#[cfg(feature = "decimal")]
impl diesel::sql_types::ops::Mul for Decimal {
    type Rhs = Decimal;
    type Output = Decimal;
}

#[cfg(feature = "decimal")]
impl diesel::sql_types::ops::Div for Decimal {
    type Rhs = Decimal;
    type Output = Decimal;
}

/// Portable array of `ST`.
///
/// - PostgreSQL: native array of the element type (`ST[]`).
/// - SQLite: the elements as a JSON array stored in `TEXT`.
///
/// Wraps another marker, e.g. `Array<diesel::sql_types::Integer>`. Unlike
/// diesel's native array, this holds the element type as `PhantomData` so the
/// marker is constructible (the `MultiBackend` bind collector needs a value).
/// The Rust value is a plain `Vec<T>`. See [`crate::types`] (the `array` module).
#[cfg(feature = "array")]
#[derive(diesel::query_builder::QueryId, diesel::sql_types::SqlType, Debug, Clone, Copy)]
pub struct Array<ST: 'static>(core::marker::PhantomData<ST>);

// Hand-written (not derived) so it doesn't pick up a spurious `ST: Default`
// bound — the bind collector constructs a marker value to dispatch on.
#[cfg(feature = "array")]
impl<ST: 'static> Default for Array<ST> {
    fn default() -> Self {
        Array(core::marker::PhantomData)
    }
}
