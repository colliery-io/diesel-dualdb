# Reference: schema generator

The `diesel-dualdb-cli` crate provides the `diesel-dualdb-schema` binary, which
reads a directory of **logical** migrations and emits per-backend migration
trees plus a unified `schema.rs`.

## CLI

```sh
cargo install diesel-dualdb-cli   # installs the `diesel-dualdb-schema` binary
diesel-dualdb-schema <migrations_dir> <out_dir>
# from a checkout of this repo instead: cargo run -p diesel-dualdb-cli -- <migrations_dir> <out_dir>
```

Exit code is non-zero on any error (unparseable DDL, unmapped type, malformed
escape-hatch directive), with a message naming the migration / table / column.

## Input layout

```
<migrations_dir>/<version>_<name>/up.sql      # required, logical DDL
<migrations_dir>/<version>_<name>/down.sql    # optional
```

Directories are processed in sorted order (timestamp prefixes sort
chronologically). An `up.sql` may contain several statements.

## Output layout

```
<out_dir>/migrations-postgres/<version>_<name>/{up,down}.sql
<out_dir>/migrations-sqlite/<version>_<name>/{up,down}.sql
<out_dir>/schema.rs
```

When a migration supplies no `down.sql`, a `DROP TABLE IF EXISTS` (reverse
order, tables created in that migration) is derived. Migrations that only
`ALTER` or `CREATE INDEX` should supply an explicit `down.sql`.

## Logical type mapping

| Logical (in `up.sql`) | PostgreSQL | SQLite | `schema.rs` marker |
|---|---|---|---|
| `UUID` | `UUID` | `BLOB` | `diesel_dualdb::sql_types::Uuid` |
| `TIMESTAMP` | `TIMESTAMPTZ` | `TEXT` | `diesel_dualdb::sql_types::Timestamp` |
| `JSON` / `JSONB` | `JSONB` | `TEXT` | `diesel_dualdb::sql_types::Json` |
| `BYTEA` | `BYTEA` | `BLOB` | `diesel_dualdb::sql_types::Bytes` |
| `TEXT` | `TEXT` | `TEXT` | `diesel::sql_types::Text` |
| `SMALLINT` | `SMALLINT` | `SMALLINT` | `diesel::sql_types::SmallInt` |
| `INTEGER` | `INTEGER` | `INTEGER` | `diesel::sql_types::Integer` |
| `BIGINT` | `BIGINT` | `BIGINT` | `diesel::sql_types::BigInt` |
| `REAL` / `FLOAT` | `REAL` | `REAL` | `diesel::sql_types::Float` |
| `BOOLEAN` | `BOOLEAN` | `INTEGER` | `diesel::sql_types::Bool` |

An unmapped type is an error. (`DATE`/`TIME`/`NUMERIC` are not yet mapped.)

## What is derived

- **Migration DDL** is rendered by rewriting only the column types and
  re-emitting the parsed statement, so foreign keys, `DEFAULT`, `UNIQUE`,
  `CHECK`, composite `PRIMARY KEY`, `CREATE INDEX`, and `ALTER TABLE` pass
  through unchanged.
- **`schema.rs`** gets a `table!` per table (portable markers, `Nullable<…>` for
  nullable columns, the PK tuple), plus `joinable!` and
  `allow_tables_to_appear_in_same_query!` derived from foreign keys.
- `ALTER TABLE` add/drop column is folded into the model, so `schema.rs`
  reflects the final state.

## Escape-hatch directives

Backend-specific raw DDL goes in a block, emitted only to that backend and
ignored by the schema model:

```sql
-- dualdb:postgres
<raw SQL>
-- dualdb:end
```

`-- dualdb:postgres` / `-- dualdb:sqlite`, closed by `-- dualdb:end`. No nesting;
unbalanced or unknown directives are errors.

## Output notes

DDL is emitted in `sqlparser`'s canonical single-line form. `schema.rs` carries a
`// @generated` header and is deterministic — commit it and treat it read-only.
