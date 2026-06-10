# How to add a portable type

A portable type is three pieces:

1. a **marker** SQL type (names the native representation per backend),
2. a **newtype** wrapping the Rust value, with `ToSql`/`FromSql` for `Pg` and
   `Sqlite`,
3. the **bridge** onto `MultiBackend` (so it works through `DualConnection`).

This guide adds a `Bytes` type (Postgres `bytea`, SQLite `BLOB`). The four
built-in types (`Uuid`, `Timestamp`, `Bytes`, `Json`) follow exactly this shape.

## 1. The marker (`src/sql_types.rs`)

```rust
#[derive(diesel::sql_types::SqlType, diesel::query_builder::QueryId, Clone, Copy, Debug)]
#[diesel(postgres_type(name = "bytea"))]
#[diesel(sqlite_type(name = "Binary"))]
pub struct Bytes;
```

## 2. The newtype + per-backend impls (`src/types/bytes.rs`)

```rust
#[derive(diesel::expression::AsExpression, diesel::deserialize::FromSqlRow, Clone, Debug, PartialEq)]
#[diesel(sql_type = crate::sql_types::Bytes)]
pub struct Bytes(pub Vec<u8>);

// Postgres: delegate to diesel's native Binary
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
// SQLite: same shape against Sqlite / SqliteValue
```

The `Pg` arm typically delegates to diesel's native support; the `Sqlite` arm
encodes into the chosen representation (here, a BLOB).

## 3. The bridge (`src/backend.rs`)

For a **non-generic** type, one line:

```rust
diesel_dualdb::bridge!(crate::sql_types::Bytes, crate::types::Bytes);
```

`bridge!` emits the three `MultiBackend` impls (`HasSqlType`, `ToSql`, `FromSql`)
that make the type work on one arm through `DualConnection`.

## Generic types

If your newtype is generic with impl-specific bounds — like `Json<T>`, whose
`MultiBackend` `ToSql` needs `T: Serialize + Debug + Send + Sync + 'static` while
`FromSql` needs only `DeserializeOwned` — `bridge!` doesn't fit. Write the three
impls by hand (see `Json` in `src/backend.rs` for the template).

## Feature-gate it

Put the type, its marker, and its bridge behind a Cargo feature so downstreams
can trim dependencies. `Bytes` is the exception — it's `std`-only, so it's always
on.

## Wire up the schema generator (optional)

To let the generator (`diesel-dualdb-cli`) recognize a new logical type, add a
row to its `map_type` table: `logical DataType → (PG native, SQLite native,
marker path)`. See the [schema generator reference](../reference/schema-gen.md).

See also: [Macros reference](../reference/macros.md) ·
[Architecture](../explanation/architecture.md).
