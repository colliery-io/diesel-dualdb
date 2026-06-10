# diesel-dualdb

**Write your Diesel query code once and run it against both PostgreSQL and SQLite.**

Targeting two backends with Diesel normally means three kinds of boilerplate:
hand-written `ToSql`/`FromSql` for the same logical types, connection/pool
dispatch plumbing, and duplicated queries even when the SQL is identical.
`diesel-dualdb` removes the first, makes the second a one-liner, and collapses
the third to a single code path — with a loud, explicit escape hatch for the
rare query that genuinely can't be shared.

It builds directly on Diesel's `#[derive(MultiConnection)]` and closes the gap
that [diesel#4079](https://github.com/diesel-rs/diesel/issues/4079) leaves open:
the derive's generated `MultiBackend` only knows Diesel's core SQL types, so
`get_result` / `RETURNING` fail for anything else (notably `Uuid`). This crate
bridges portable types onto `MultiBackend`, so write-once queries — inserts with
`RETURNING` included — work on a single arm against either backend.

> Built on Diesel 2.3, MSRV 1.86. Sync — see [Async](#async).

📖 **Full documentation lives in [`docs/`](https://github.com/colliery-io/diesel-dualdb/blob/main/docs/index.md)** —
[Diátaxis](https://diataxis.fr/)-structured tutorials, how-to guides, reference,
and design explanation. New here? Start with the
[getting-started tutorial](https://github.com/colliery-io/diesel-dualdb/blob/main/docs/tutorials/getting-started.md).

## Quick look

One function, both backends, no per-backend `match`:

```rust
use diesel::prelude::*;
use diesel_dualdb::{types::Uuid, DualConnection};

pub fn insert_task(conn: &mut DualConnection, new: &NewTask) -> QueryResult<Task> {
    diesel::insert_into(tasks::table)
        .values(new)
        .get_result(conn) // RETURNING works on Postgres *and* SQLite
}
```

Connect with one call — the backend is detected from the URL:

```rust
let pool = diesel_dualdb::Pool::connect("postgres://localhost/app")?; // or "app.db", ":memory:"
let mut conn = pool.get()?;
```

## Portable types

Each type is a marker SQL type (`diesel_dualdb::sql_types::*`) for your `table!`
plus a value wrapper (`diesel_dualdb::types::*`). The marker resolves to the
right native type per backend; the value round-trips identically through a
`DualConnection`.

| Logical | PostgreSQL | SQLite | Value | Feature |
|---|---|---|---|---|
| UUID | `uuid` | `BLOB` (16 bytes) | `types::Uuid` | `uuid` |
| Timestamp | `timestamptz` | `TEXT` (RFC3339, UTC) | `types::Timestamp` | `chrono` |
| Binary | `bytea` | `BLOB` | `types::Bytes` | (always) |
| JSON | `jsonb` | `TEXT` | `types::Json<T>` | `serde_json` |
| Decimal | `numeric` | `TEXT` | `types::Decimal` | `decimal` |
| Array | `T[]` | `TEXT` (JSON) | `types::Array<T>` | `array` |

Each feature also enables Diesel's matching integration; the default set is all
of them. **Enums** are portable too — derive [`DualEnum`](https://github.com/colliery-io/diesel-dualdb/blob/main/docs/reference/macros.md#derivedualenum)
on a fieldless enum to map it to a PostgreSQL native `enum` / SQLite `TEXT`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, diesel_dualdb::DualEnum)]
#[dualdb(pg_type = "mood")]
pub enum Mood { Happy, Sad, #[dualdb(rename = "meh")] Neutral }
```

> **Required Diesel feature:** `returning_clauses_for_sqlite_3_35` (non-optional)
> — one-arm `RETURNING` needs both backends to support a `RETURNING` clause.

## Schema & migrations

Write your schema **once** in *logical* DDL (logical column types like `UUID`,
`TIMESTAMP`, `JSON`); the `diesel-dualdb-schema` tool (the `diesel-dualdb-cli`
crate) produces both backends' migration trees and a unified, portably-typed
`schema.rs`:

```sql
-- schema/migrations/2026-01-01-000000_init/up.sql
CREATE TABLE widgets (
    id UUID PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    meta JSON,
    created_at TIMESTAMP NOT NULL
);
```

```sh
cargo install diesel-dualdb-cli   # installs the `diesel-dualdb-schema` binary
diesel-dualdb-schema schema/migrations schema/generated
# -> schema/generated/{schema.rs, migrations-postgres/, migrations-sqlite/}
```

The generated trees are ordinary Diesel migrations — apply them per backend with
`diesel migration run --migration-dir …`. Foreign keys become `joinable!` +
`allow_tables_to_appear_in_same_query!`, composite primary keys become
`table! (a, b)`, and a `-- dualdb:postgres … -- dualdb:end` block is the escape
hatch for backend-specific DDL. See
[the schema how-to](https://github.com/colliery-io/diesel-dualdb/blob/main/docs/how-to/generate-schema-and-migrations.md).

## Testing both backends

`#[diesel_dualdb::test]` expands one body into a test per backend, each handed a
`&mut DualConnection`. SQLite runs in-memory; Postgres runs against
`DUALDB_PG_URL` and skips cleanly when it's unset.

```rust
#[diesel_dualdb::test(pg, sqlite)]
fn uuid_round_trips(conn: &mut DualConnection) {
    // identical assertions; both backends must pass
}
```

```sh
cargo test                             # SQLite arm runs; Postgres arm self-skips
DUALDB_PG_URL=postgres://… cargo test  # run both arms
```

## When backends must diverge

Most code stays on one arm. For SQL that genuinely can't be shared (locking
hints, recursive CTEs, `ON CONFLICT` upserts), diverge **explicitly** — the
compiler enforces both arms and it's visible at the call site:

```rust
conn.dispatch(
    |pg|     claim_ready_pg(pg, limit),
    |sqlite| claim_ready_sqlite(sqlite, limit),
)
```

See [Diverge per backend](https://github.com/colliery-io/diesel-dualdb/blob/main/docs/how-to/diverge-per-backend.md) for when (and when
not) to reach for this.

## Adding your own portable type

A type is three pieces: a **marker** SQL type, a **newtype** with `ToSql`/`FromSql`
for `Pg` and `Sqlite`, and the **bridge** onto `MultiBackend` — one line via the
`bridge!` macro for non-generic types:

```rust
diesel_dualdb::bridge!(crate::sql_types::Bytes, crate::types::Bytes);
```

Full walkthrough: [How to add a portable type](https://github.com/colliery-io/diesel-dualdb/blob/main/docs/how-to/add-a-portable-type.md).

## Async

Sync only. To use it from an async runtime, wrap the sync calls in
`tokio::task::spawn_blocking` (the same model `diesel-async` uses for SQLite); a
native `AsyncDualConnection` is not provided.

## Development

Working on the crate itself? The repo uses `angreal` as its task runner —
`angreal test all` (SQLite + a disposable Postgres via docker-compose),
`angreal check all` (feature matrix + fmt + clippy), and `angreal schema gen` —
plus docker-compose for a throwaway Postgres. These are contributor tooling, not
part of the published crate.

## License

Dual-licensed under either [MIT](https://opensource.org/license/mit) or
[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0) at your option.
