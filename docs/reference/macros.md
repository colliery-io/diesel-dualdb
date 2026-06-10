# Reference: macros

Both macros live in the `diesel-dualdb-macros` crate and are re-exported from
`diesel_dualdb`.

## `#[diesel_dualdb::test]`

Attribute macro. Applied to `fn name(conn: &mut DualConnection) { … }`, it keeps
your function and generates one `#[test]` per selected backend.

```rust
#[diesel_dualdb::test(pg, sqlite)]
fn name(conn: &mut DualConnection) { … }
```

| Form | Generates |
|---|---|
| `#[diesel_dualdb::test]` | both `name_sqlite` and `name_pg` |
| `#[diesel_dualdb::test(sqlite)]` | `name_sqlite` only |
| `#[diesel_dualdb::test(pg)]` | `name_pg` only |
| `#[diesel_dualdb::test(pg, sqlite)]` | both |

- `name_sqlite` opens an in-memory `SqliteConnection`.
- `name_pg` connects to the `DUALDB_PG_URL` environment variable; if it is unset
  the test prints a skip notice and returns (it does not fail).
- Attributes on the function (e.g. `#[should_panic]`) are forwarded to the
  generated tests.
- An unknown backend argument is a compile error.

## `bridge!`

Function-like macro. Generates the three `MultiBackend` bridge impls for a
**non-generic** portable type.

```rust
diesel_dualdb::bridge!(crate::sql_types::Marker, crate::types::Newtype);
```

Expands to:

```rust
impl HasSqlType<Marker> for MultiBackend { /* lookup_sql_type */ }
impl ToSql<Marker, MultiBackend> for Newtype { /* set_value */ }
impl FromSql<Marker, MultiBackend> for Newtype { /* from_sql */ }
```

Requirements and notes:

- The per-backend `ToSql`/`FromSql<_, Pg>` and `<_, Sqlite>` for the newtype must
  already exist (they live with the type).
- Gate the call with the same `#[cfg(feature = …)]` as the type.
- Usable from external crates too (to bridge your own types onto `MultiBackend`).
- **Generic** newtypes with impl-specific bounds (e.g. `Json<T>`) don't fit;
  write their three impls by hand.

## `#[derive(DualEnum)]`

Derive macro. Turns a fieldless Rust enum into a portable enum — PostgreSQL
native `enum`, SQLite `TEXT` (the variant label) — working through
`DualConnection`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, diesel_dualdb::DualEnum)]
#[dualdb(pg_type = "mood")]
pub enum Mood {
    Happy,
    Sad,
    #[dualdb(rename = "meh")]
    Neutral,
}
```

Generates a marker SQL type **`MoodSqlType`** (`<EnumName>SqlType`) — use it in
`table!`, e.g. `feeling -> MoodSqlType` — plus all the
`ToSql`/`FromSql`/`AsExpression`/`Queryable` impls and the `MultiBackend` bridge.

| Attribute | On | Meaning | Default |
|---|---|---|---|
| `#[dualdb(pg_type = "name")]` | enum | PostgreSQL enum type name | enum name, lowercased |
| `#[dualdb(rename = "label")]` | variant | stored label | the variant identifier |

Notes:

- The enum must derive `Debug` (`ToSql` requires it); derive `Clone` too.
- Variants must be **unit** (fieldless).
- Your **migration** creates the type: on Postgres `CREATE TYPE name AS ENUM (...)`
  (a `CREATE TYPE` isn't derivable, so it goes in your migration / the
  generator's escape hatch); on SQLite the column is `TEXT` (optionally with a
  `CHECK`).
- The schema generator doesn't emit enum columns — declare them by hand.

See: [How to add a portable type](../how-to/add-a-portable-type.md).
