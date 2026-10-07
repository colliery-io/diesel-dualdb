# Explanation: async

`AsyncDualConnection` is the async counterpart of `DualConnection`. This page
explains how it is built, and why it is built that way.

## The constraint: one backend

The type layer and the bridge are written against the `MultiBackend` that
`#[derive(MultiConnection)]` generates. To reuse them unchanged, an async
connection must also have `Backend = MultiBackend`. diesel-async has no async
`MultiConnection`, so the connection must be built in this crate.

## Options that were considered

| Option | Why not chosen |
|---|---|
| `SyncConnectionWrapper<DualConnection>` | Blocked. The wrapper needs a `MoveableBindCollector` and an `IntoOwnedRow` row type. The derive generates neither, and its bind collector holds borrowed values that cannot be moved. |
| Upstream-first: add those traits to the derive | Would need **two** diesel PRs (the derive, and `IntoOwnedRow` for `PgRow`), and the work could not ship until both were released. Postgres would also run on `spawn_blocking`. |
| Reimplement the derive in this crate | A large fork to maintain, and Postgres would still run on `spawn_blocking`. |
| **Hand-written enum (chosen)** | Ships in this crate with no upstream dependency, and Postgres runs natively async. |

## How it works

```rust
pub enum AsyncDualConnection {
    Pg(diesel_async::AsyncPgConnection),                                   // tokio-postgres
    Sqlite(diesel_async::sync_connection_wrapper::SyncConnectionWrapper<SqliteConnection>), // spawn_blocking
}
```

It implements diesel-async's `AsyncConnection` with `Backend = MultiBackend`.
A query is a `QueryFragment<MultiBackend>`. To run it on an arm, the query is
wrapped in an `ArmQuery`, a `QueryFragment<Pg>` or `QueryFragment<Sqlite>` that:

1. renders the SQL with that arm's `MultiBackend` query builder;
2. collects the binds with that arm's `MultiBackend` bind collector, and gives
   them to the arm's own collector;
3. passes debug output through, for logging and `debug_query`.

These are the same steps as the derive's private `SerializedQuery` for the sync
connection. The derive's query-builder and bind-collector enums are in private
modules, but the projections `<MultiBackend as Backend>::QueryBuilder` and
`::BindCollector` can name them and their variants. Rows come back as
`AsyncDualRow`, an enum over the arms' rows. Its fields return the public
`MultiRawValue`, so `FromSql<_, MultiBackend>` decodes them as usual.

Both arms turn a query into SQL and binds before the first `.await`. So the
query does not need to be `Send`, and the futures hold only owned data.

## Why the Postgres arm of `DualConnection` is a newtype

To collect a Postgres bind, `MultiBackend` gets the type-metadata lookup from a
`&mut dyn Any` through `MultiConnectionHelper::from_any`. With a
`PgConnection` arm, that downcast accepts only a sync `PgConnection`.
diesel-async uses its own lookup, so with a plain `PgConnection` arm, every
async Postgres bind would panic.

The Postgres arm of `DualConnection` is therefore `DualPgConnection`, a thin
newtype over `PgConnection`. Its `from_any` accepts the sync connection
**and** diesel-async's lookup. The async lookup is passed in a wrapper that
lives for one `collect_binds` call only. The lookup matters for types whose OID
is not fixed: a Postgres `enum` from `DualEnum` is resolved this way.
`DualPgConnection` delegates everything to the inner connection and derefs to
`PgConnection`, so sync code that matches `DualConnection::Pg(c)` still
compiles in most cases.

## What stays sync-only

- `on_conflict`: the `MultiBackend` dialect does not support it, for sync and
  async alike. Use it on the arm (see [How to use async](../how-to/use-async.md#diverge-per-backend)).
- The schema generator is a build-time tool and has no async form.

See also: [Architecture](architecture.md).
