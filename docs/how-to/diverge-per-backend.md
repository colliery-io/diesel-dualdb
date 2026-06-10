# How to diverge per backend

Most queries and DDL are shared. When something genuinely cannot be expressed
the same way on both backends, diverge **explicitly** — loudly, in one place.
There is no hidden third path.

## In queries: match on the connection

When SQL can't be shared (locking hints, recursive CTEs, FTS, upsert feature
gaps), match the connection enum and call a per-backend function:

```rust
pub fn claim_ready(conn: &mut DualConnection, limit: i64) -> QueryResult<Vec<Job>> {
    match conn {
        DualConnection::Pg(c)     => claim_ready_pg(c, limit),     // CTE + FOR UPDATE SKIP LOCKED
        DualConnection::Sqlite(c) => claim_ready_sqlite(c, limit), // BEGIN IMMEDIATE strategy
    }
}
```

The `match` is exhaustive — the compiler makes you handle both arms — and it's
visible at the call site. Keep these rare; reach for one only when the shared
path genuinely can't express the query.

### Upsert (`ON CONFLICT`) is one of these cases

Both backends support `ON CONFLICT … DO UPDATE / DO NOTHING`, but diesel's
`MultiConnection` derive sets `MultiBackend`'s on-conflict dialect to
"unsupported" (and, unlike `RETURNING`, there's no feature to enable it). So
`on_conflict` won't compile through `DualConnection` — run it on the concrete
connection. The query is identical on both arms, so a macro keeps it DRY:

```rust
macro_rules! upsert { ($c:expr) => {
    diesel::insert_into(kv::table)
        .values((kv::k.eq("a"), kv::v.eq(2)))
        .on_conflict(kv::k).do_update().set(kv::v.eq(2))
        .execute($c)
}}

match conn {
    DualConnection::Pg(c)     => { upsert!(c)?; }
    DualConnection::Sqlite(c) => { upsert!(c)?; }
}
```

### A tidier helper: `dispatch`

`DualConnection::dispatch` is the same two-arm split as a method — both closures
are required (so divergence stays exhaustive), and it returns a value:

```rust
let rows = conn.dispatch(
    |pg|     upsert!(pg),
    |sqlite| upsert!(sqlite),
)?;
```

Reach for it when a `match` would be noisier; it makes the divergence readable,
not hidden.

## In schema/DDL: the escape-hatch block

When a migration needs backend-specific DDL (a GIN index, a partial index, a
`CHECK` that differs, `WITHOUT ROWID`), wrap it in a directive block. It's
emitted **verbatim to that backend only** and ignored by `schema.rs`:

```sql
CREATE TABLE docs (
    id UUID PRIMARY KEY NOT NULL,
    body JSON NOT NULL
);

-- dualdb:postgres
CREATE INDEX docs_body_gin ON docs USING gin (body);
-- dualdb:end

-- dualdb:sqlite
CREATE INDEX docs_body_idx ON docs (body);
-- dualdb:end
```

The Postgres migration gets the GIN index; the SQLite migration gets the plain
index; neither gets the other.

### Rules

- Blocks are `-- dualdb:postgres` / `-- dualdb:sqlite`, closed by `-- dualdb:end`.
- Blocks can't nest; an unclosed or unknown directive is an error.
- The escape-hatch SQL is **not** parsed — it's passed through, so it can be
  anything that backend accepts.
- It does **not** feed the schema model, so don't create columns there that your
  `table!` needs.

See also: [Design philosophy](../explanation/design-philosophy.md) ·
[Generate schema & migrations](generate-schema-and-migrations.md).
