# Getting started

In this tutorial you'll build a tiny module that stores and reads a `widget`
row, and you'll watch the **same code** run on both PostgreSQL and SQLite. By
the end you'll have written one schema, generated both backends' migrations, and
seen a test pass on each.

You'll need Rust (1.86+), and Docker if you want to exercise the Postgres path.
SQLite needs nothing.

## 1. Add the dependency and features

In your `Cargo.toml`:

```toml
[dependencies]
diesel-dualdb = { version = "0", features = ["uuid", "chrono", "serde_json"] }
diesel = { version = "2.3", features = ["postgres", "sqlite", "r2d2", "returning_clauses_for_sqlite_3_35"] }
uuid = "1"
chrono = "0.4"
serde_json = "1"
```

The `returning_clauses_for_sqlite_3_35` diesel feature is **required** — it's
what lets `RETURNING` work on the SQLite arm. (See
[Cargo features & MSRV](../reference/features-and-msrv.md).)

## 2. Write your schema, once, in logical DDL

Create `schema/migrations/2026-01-01-000000_init/up.sql`. Use **logical** column
types — `UUID`, `TIMESTAMP`, `JSON` — not a specific backend's spelling:

```sql
CREATE TABLE widgets (
    id UUID PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    meta JSON,
    created_at TIMESTAMP NOT NULL
);
```

## 3. Generate the per-backend migrations and `schema.rs`

```sh
cargo install diesel-dualdb-cli   # one-time: installs the `diesel-dualdb-schema` binary
diesel-dualdb-schema schema/migrations schema/generated
```

You now have three things under `schema/generated/`:

- `migrations-postgres/…/up.sql` — `id UUID …, meta JSONB, created_at TIMESTAMPTZ`
- `migrations-sqlite/…/up.sql` — `id BLOB …, meta TEXT, created_at TEXT`
- `schema.rs` — one `table!`, typed with portable markers:

```rust
diesel::table! {
    use diesel::sql_types::{Nullable, Text};
    use diesel_dualdb::sql_types::{Json, Timestamp, Uuid};

    widgets (id) {
        id -> Uuid,
        name -> Text,
        meta -> Nullable<Json>,
        created_at -> Timestamp,
    }
}
```

Bring it into your crate (for example `include!("../schema/generated/schema.rs");`).

## 4. Write a query — once

Note the signature: `&mut DualConnection`. No per-backend branching.

```rust
use diesel::prelude::*;
use diesel_dualdb::types::{Json, Timestamp, Uuid};
use diesel_dualdb::DualConnection;

pub fn insert_widget(conn: &mut DualConnection, name: &str) -> QueryResult<Uuid> {
    diesel::insert_into(widgets::table)
        .values((
            widgets::id.eq(Uuid(uuid::Uuid::new_v4())),
            widgets::name.eq(name),
            widgets::meta.eq(None::<Json<serde_json::Value>>),
            widgets::created_at.eq(Timestamp(chrono::Utc::now())),
        ))
        .returning(widgets::id)
        .get_result(conn) // RETURNING works on Postgres *and* SQLite
}
```

## 5. Run it on both backends from one test

Write the test once; `#[diesel_dualdb::test]` runs it per backend, handing each a
`&mut DualConnection`:

```rust
use diesel::connection::SimpleConnection;

#[diesel_dualdb::test(pg, sqlite)]
fn widget_round_trips(conn: &mut DualConnection) {
    // create the table from the generated migration for this backend
    let up = match &*conn {
        DualConnection::Pg(_) => include_str!(
            "../schema/generated/migrations-postgres/2026-01-01-000000_init/up.sql"),
        DualConnection::Sqlite(_) => include_str!(
            "../schema/generated/migrations-sqlite/2026-01-01-000000_init/up.sql"),
    };
    conn.batch_execute(up).unwrap();

    let id = insert_widget(conn, "gizmo").unwrap();

    let name: String = widgets::table
        .select(widgets::name)
        .filter(widgets::id.eq(id))
        .first(conn)
        .unwrap();
    assert_eq!(name, "gizmo");
}
```

## 6. Run the tests

```sh
cargo test                  # SQLite arm runs; the Postgres arm self-skips
DUALDB_PG_URL=postgres://… cargo test    # both arms run
```

The SQLite arm runs in-memory with no setup. The Postgres arm runs only when
`DUALDB_PG_URL` points at a database, and otherwise prints a skip notice.

## What you did

You wrote **one** schema and **one** query function, and the same code stored
and read a row on two different databases. That's the whole idea.

Next:
- [Generate schema & migrations](../how-to/generate-schema-and-migrations.md) in more depth.
- [Add a portable type](../how-to/add-a-portable-type.md) of your own.
- [Architecture](../explanation/architecture.md) — how the one-arm trick works.
