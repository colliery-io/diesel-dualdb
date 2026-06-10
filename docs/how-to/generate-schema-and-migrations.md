# How to generate schema & migrations

Write your schema once in **logical DDL**; generate both backends' migrations
and a unified `schema.rs` from it.

## Lay out logical migrations

Use diesel's migration layout, but with logical column types:

```
schema/migrations/
  2026-01-01-000000_init/
    up.sql        # logical CREATE TABLE …
    down.sql      # optional; auto-derived (DROP TABLE) if omitted
```

```sql
-- up.sql
CREATE TABLE users (
    id UUID PRIMARY KEY NOT NULL,
    email TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL
);
```

## Generate

```sh
cargo install diesel-dualdb-cli   # installs the `diesel-dualdb-schema` binary
diesel-dualdb-schema schema/migrations schema/generated
```

Output under `schema/generated/`:

- `migrations-postgres/<name>/{up,down}.sql` — native `uuid`/`timestamptz`/`jsonb`/`bytea`
- `migrations-sqlite/<name>/{up,down}.sql` — native `BLOB`/`TEXT`
- `schema.rs` — one `table!` per table, portable markers, `joinable!` for FKs

Commit the generated output and treat it as read-only.

## Apply the migrations

The generated trees are ordinary diesel migrations:

```sh
diesel migration run --migration-dir schema/generated/migrations-postgres --database-url "$POSTGRES_URL"
diesel migration run --migration-dir schema/generated/migrations-sqlite   --database-url app.db
```

## Use the schema in code

```rust
include!("../schema/generated/schema.rs");   // brings the table! modules into scope
```

## Relations

A foreign key generates the diesel join wiring automatically:

```sql
CREATE TABLE posts (
    id UUID PRIMARY KEY NOT NULL,
    author UUID NOT NULL REFERENCES users(id)
);
```

→ in `schema.rs`:

```rust
diesel::joinable!(posts -> users (author));
diesel::allow_tables_to_appear_in_same_query!(users, posts);
```

A composite primary key becomes `table! (a, b)`.

## Keep it fresh

Commit `schema/generated/` and add a CI step that regenerates into a temp dir
and fails if it differs from what's committed, so the generated files can't
drift:

```sh
diesel-dualdb-schema schema/migrations /tmp/schema-check
diff -r /tmp/schema-check schema/generated
```

See also: [Schema generator reference](../reference/schema-gen.md) ·
[Diverge per backend](diverge-per-backend.md) ·
[Why a logical schema](../explanation/schema-generation.md).
