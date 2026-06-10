# How to test on both backends

Use `#[diesel_dualdb::test]` to run one test body against PostgreSQL and SQLite.

## Run a test on both

```rust
use diesel_dualdb::DualConnection;

#[diesel_dualdb::test(pg, sqlite)]
fn it_round_trips(conn: &mut DualConnection) {
    // identical assertions; both backends must pass
}
```

This expands into two `#[test]` functions, `it_round_trips_sqlite` and
`it_round_trips_pg`. Each builds a `DualConnection` and calls your body.

- **SQLite** runs in-memory (`:memory:`), no setup.
- **Postgres** connects to `DUALDB_PG_URL`. If that env var is unset, the
  Postgres test prints a skip notice and returns (it does **not** fail), so the
  suite is green on a machine with no Postgres.

## Pick one backend

```rust
#[diesel_dualdb::test(sqlite)]      // SQLite only
#[diesel_dualdb::test(pg)]          // Postgres only
#[diesel_dualdb::test]              // both (same as (pg, sqlite))
```

## Create the schema inside the test

The body gets a connection, not a database. Create your tables first — for type
round-trips, the one legitimately per-backend bit is the DDL, so match the arm
just to pick it:

```rust
use diesel::connection::SimpleConnection;

fn create_table(conn: &mut DualConnection) {
    let ddl = match &*conn {
        DualConnection::Pg(_)     => "CREATE TEMP TABLE t (id UUID PRIMARY KEY NOT NULL);",
        DualConnection::Sqlite(_) => "CREATE TABLE t (id BLOB PRIMARY KEY NOT NULL);",
    };
    conn.batch_execute(ddl).expect("create table");
}
```

(Or feed the test the migrations from the
[schema generator](generate-schema-and-migrations.md).)

## Forward attributes

Attributes on the function are forwarded to the generated tests:

```rust
#[diesel_dualdb::test(sqlite)]
#[should_panic]
fn rejects_bad_input(conn: &mut DualConnection) { /* … */ }
```

## Run the suite

```sh
cargo test                             # SQLite arm runs; Postgres arm self-skips
DUALDB_PG_URL=postgres://… cargo test  # run both arms
```

You provide the Postgres for the `pg` arm via `DUALDB_PG_URL`. (This repository's
own development tasks wrap that — `angreal test all` spins up a throwaway
Postgres container — but that's contributor tooling, not part of the crate.)

See also: [Macros reference](../reference/macros.md).
