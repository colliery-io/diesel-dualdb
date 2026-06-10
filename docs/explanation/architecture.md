# Explanation: architecture

The crate is two layers with different runtime coupling.

## Layer A — portable types (runtime-agnostic)

`ToSql`/`FromSql` are defined against a `Backend`, not a connection, so this
layer is identical for sync and async, and is the reusable heart of the crate.

Each logical type is two pieces:

- a **marker** SQL type (`dualdb::sql_types::*`), annotated with its native
  representation per backend (`#[diesel(postgres_type(...))]`,
  `#[diesel(sqlite_type(...))]`);
- a **domain newtype** (`dualdb::types::*`) wrapping the Rust value, with
  `ToSql`/`FromSql` for `Pg` and for `Sqlite`.

## Layer B — the connection and the bridge

The crate owns the canonical connection enum, so it also owns `MultiBackend` and
can implement the bridge traits locally — no orphan-rule problem:

```rust
#[derive(diesel::MultiConnection)]
pub enum DualConnection {
    Pg(diesel::PgConnection),
    Sqlite(diesel::SqliteConnection),
}
```

For each portable type, the bridge implements three traits against the generated
`MultiBackend`:

- `HasSqlType<Marker>` — so `QueryMetadata` is satisfied and the query builder
  knows the type exists on the unified backend. **This is the hole #4079 leaves
  open**; closing it is what makes one-arm `get_result`/`RETURNING` work.
- `ToSql<Marker, MultiBackend>` — dispatches binding to the active inner backend.
- `FromSql<Marker, MultiBackend>` — dispatches deserialization off the active
  inner backend's raw value.

### Why the bridge is a one-liner

The feared part — matching the generated raw-value enum in `FromSql` — turns out
to be solved by the derive itself. `#[derive(MultiConnection)]` emits two public
helpers that do the per-arm dispatch:

- `MultiBackend::lookup_sql_type::<ST>(lookup)` — backs `HasSqlType`;
- `MultiRawValue::from_sql::<T, ST>()` — backs `FromSql` (you never match the raw
  value enum yourself).

So each bridge impl is a single delegation, *identical in shape* to what the
derive generates for the core types. That's why the [`bridge!`](../reference/macros.md)
macro can generate them, and why a non-generic type's bridge is one line.

## Where things live

```
src/
  lib.rs           DualConnection (#[derive(MultiConnection)]) + re-exports
  sql_types.rs     portable markers
  types/           uuid/bytes/timestamp/json/decimal/array .rs  (per-backend ToSql/FromSql)
  backend.rs       the MultiBackend bridge (bridge! lines + the hand-written Json block)
diesel-dualdb-macros/   #[diesel_dualdb::test], bridge!, #[derive(DualEnum)]
diesel-dualdb-cli/      the logical-DDL → migrations + schema.rs generator (binary)
```

## A required feature, for a structural reason

`get_result`/`RETURNING` through `MultiBackend` needs *both* arms to satisfy the
`RETURNING`-clause query fragment. Diesel gates SQLite's RETURNING support behind
`returning_clauses_for_sqlite_3_35`, so that feature is **non-optional** for the
crate. (See [Cargo features](../reference/features-and-msrv.md).)

## The connection is local — so no orphan problem

Because `DualConnection` (and therefore `MultiBackend`) is defined in this crate,
the bridge impls are local impls on local types. A downstream crate can do the
same for its own types via `bridge!`, because `MultiBackend` is re-exported.
