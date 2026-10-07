# How to use diesel-dualdb from async code

`AsyncDualConnection` runs the same write-once queries as `DualConnection`, but
through [`diesel-async`](https://docs.rs/diesel-async): PostgreSQL natively
async, SQLite on `spawn_blocking`. Every portable type works unchanged.

## Turn it on

```toml
[dependencies]
diesel-dualdb = { version = "0.1", features = ["async"] }
diesel-async = "0.9"   # for RunQueryDsl / AsyncConnection
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Connect

`establish` picks the backend from the URL, with the same rules as
[`Pool::connect`](pool-connections.md):

```rust
use diesel_async::AsyncConnection;
use diesel_dualdb::AsyncDualConnection;

let mut conn = AsyncDualConnection::establish("postgres://localhost/app").await?;
// or "app.db", "sqlite://app.db", ":memory:", "file:…"
```

## Query

Write the query exactly as for `DualConnection`, import `RunQueryDsl` from
`diesel_async` instead of `diesel`, and `.await` it:

```rust
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

pub async fn insert_task(
    conn: &mut AsyncDualConnection,
    new: &NewTask,
) -> QueryResult<Task> {
    diesel::insert_into(tasks::table)
        .values(new)
        .get_result(conn) // RETURNING works on Postgres *and* SQLite
        .await
}
```

Transactions use diesel-async's `transaction`, and nested calls become savepoints:

```rust
conn.transaction(async |conn| {
    diesel::insert_into(tasks::table).values(new).execute(conn).await?;
    diesel::update(counts::table).set(counts::n.eq(counts::n + 1)).execute(conn).await
})
.await?;
```

DDL and other raw SQL go through `diesel_async::SimpleAsyncConnection::batch_execute`.

## Pool

`AsyncPool` is the async `Pool`: a deadpool pool with the same URL detection.

```rust
use diesel_dualdb::AsyncPool;

let pool = AsyncPool::builder()
    .max_size(16)
    .connection_timeout(std::time::Duration::from_secs(5))
    .connect(&database_url)?;      // checks the URL scheme only
let mut conn = pool.get().await?;  // opens a connection on first use
// use `&mut *conn` anywhere a `&mut AsyncDualConnection` is wanted
```

deadpool opens connections on demand, so an unreachable database is reported by
the first `get()`, not by `connect`. The `:memory:` caveat from
[Pool connections](pool-connections.md#sqlite--pooling) applies here too.

## Test on both backends

`#[diesel_dualdb::test]` works on an `async fn`. Each generated test opens an
`AsyncDualConnection` and awaits the body on its own tokio runtime, so there's no
`#[tokio::test]`:

```rust
#[diesel_dualdb::test(pg, sqlite)]
async fn inserts_a_task(conn: &mut AsyncDualConnection) {
    // diesel_async::RunQueryDsl queries, awaited; same assertions on both
}
```

As for sync tests, the Postgres test reads `DUALDB_PG_URL` and skips when it is unset.

## Diverge per backend

Match on the arm. Each arm is a plain diesel-async connection
(`AsyncPgConnection` and `SyncConnectionWrapper<SqliteConnection>`), so
backend-specific SQL, such as `ON CONFLICT` upserts, runs there:

```rust
match conn {
    AsyncDualConnection::Pg(pg) => upsert!(pg).await?,
    AsyncDualConnection::Sqlite(sqlite) => upsert!(sqlite).await?,
};
```

See [Diverge per backend](diverge-per-backend.md) for when to reach for this.

See also: [Explanation: async](../explanation/async.md).
