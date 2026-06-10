# How to pool connections

## The one-liner

`Pool::connect` detects the backend from the URL and builds an r2d2 pool:

```rust
use diesel_dualdb::Pool;

let pool = Pool::connect("postgres://localhost/app")?;   // or "app.db", ":memory:", "file:…"
let mut conn = pool.get()?;                               // a pooled DualConnection
// use `&mut *conn` anywhere a `&mut DualConnection` is wanted
```

Detection:

| URL | Backend |
|---|---|
| `postgres://…`, `postgresql://…` | PostgreSQL |
| `sqlite://…`, `file:…`, `:memory:`, a bare path (`app.db`) | SQLite |
| any other scheme (`mysql://…`) | error (`Pool::Error::UnknownUrl`) |

Unlike the connection's derived `establish` (which tries Postgres, then SQLite),
`Pool::connect` establishes exactly the detected backend.

## Configure the pool

```rust
let pool = Pool::builder()
    .max_size(16)
    .min_idle(Some(2))
    .connect(&database_url)?;
```

For r2d2 settings not surfaced on the builder, reach the underlying pool via
`pool.inner()`, or build your own over the public `pool::DualConnectionManager`.

## SQLite + pooling

A `:memory:` database is **per connection**, so a multi-connection pool over
`:memory:` gives each checkout its own empty database. For pooled SQLite, use a
file (or `file::memory:?cache=shared`), or cap the pool at `max_size(1)`.

See also: [Architecture](../explanation/architecture.md).
