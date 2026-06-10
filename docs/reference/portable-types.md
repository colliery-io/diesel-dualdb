# Reference: portable types

Each portable type is a **marker** (`diesel_dualdb::sql_types::*`) used in
`table!` definitions, and a **newtype** (`diesel_dualdb::types::*`) wrapping the
Rust value. The marker resolves to a different native type per backend.

| Marker / newtype | Rust value | PostgreSQL | SQLite | Feature |
|---|---|---|---|---|
| `Uuid` | `uuid::Uuid` | `uuid` | `BLOB` (16 bytes) | `uuid` |
| `Timestamp` | `chrono::DateTime<Utc>` | `timestamptz` | `TEXT` (RFC3339) | `chrono` |
| `Bytes` | `Vec<u8>` | `bytea` | `BLOB` | (always on) |
| `Json<T>` | `T: Serialize + DeserializeOwned` | `jsonb` | `TEXT` | `serde_json` |
| `Decimal` | `bigdecimal::BigDecimal` | `numeric` | `TEXT` | `decimal` |
| `Array<ST>` | `types::Array<T>` (wraps `Vec<T>`) | `ST[]` (native) | `TEXT` (JSON) | `array` |

Each implements `ToSql`/`FromSql` for `Pg`, `Sqlite`, and the generated
`MultiBackend`, so they work through a `DualConnection` on one arm.

## Notes per type

### `Uuid`
Stored as a 16-byte `BLOB` on SQLite (not TEXT) to keep index size small.
`From<uuid::Uuid>` / `Into<uuid::Uuid>` provided.

### `Timestamp`
SQLite uses a **fixed** RFC3339 format — UTC, `Z` suffix, 6 fractional digits —
so lexicographic ordering equals chronological ordering. Precision is
microseconds on both backends; finer precision is truncated. See
[Timestamp representation](../explanation/timestamp-representation.md).

### `Bytes`
Delegates to diesel's native `Binary` on both backends. Distinguishes an empty
blob (`Bytes(vec![])`) from SQL `NULL` (`Option<Bytes>`).

### `Json<T>`
Generic over any `serde`-serializable `T` (e.g. a typed struct, or
`serde_json::Value`). Postgres uses the native `jsonb` wire format (the `0x01`
version byte); SQLite stores serialized JSON `TEXT`. Through `MultiBackend`, `T`
must additionally be `Send + Sync + 'static` (the bind collector boxes the value
across the backend enum).

### `Decimal`
Wraps `bigdecimal::BigDecimal`. Postgres uses native `numeric`; SQLite stores the
canonical decimal **string** as `TEXT`, an exact round-trip. SQLite-side
arithmetic/ordering on that text is **not** guaranteed — treat it as a value
store and do decimal math in Rust. (Column precision/scale, e.g. `NUMERIC(10,2)`,
isn't enforced on the SQLite side.)

### `Array<ST>`
The marker `sql_types::Array<ST>` wraps an element SQL type; the value is
`types::Array<T>` (a `Vec<T>` newtype) where `T` is the element's Rust type, e.g.
a column `tags -> Array<Text>` carries `types::Array<String>`. Postgres uses a
native array (`text[]`); SQLite stores a JSON array as `TEXT`, so SQLite-side
array operators aren't available (it's a JSON value there). Elements must be
`Serialize + DeserializeOwned` (for the SQLite JSON) and implement the element's
`ToSql`/`FromSql` (for the PG array). The schema generator doesn't emit array
columns yet — declare them in a hand-written `table!` / migration.

## Enums

A fieldless Rust enum becomes a portable enum (PostgreSQL native `enum`, SQLite
`TEXT`) with `#[derive(DualEnum)]` — see [Macros](macros.md#derivedualenum). It
generates a `<Enum>SqlType` marker for your `table!`.

## Nullability

A nullable column is `Option<T>` / `Nullable<Marker>`, as in any Diesel schema.
