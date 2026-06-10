# Explanation: schema generation

## Why not `diesel print-schema`?

`diesel print-schema` reads a **live database** and emits that backend's
**native** SQL types. For a dual-backend project that breaks two ways:

1. **It runs against one backend at a time.** Point it at Postgres and a column
   prints as `Uuid`; point it at SQLite and the same logical column (stored as a
   `BLOB`) prints as `Binary`. The two backends produce *different* schema files.
   No single run yields a unified schema.
2. **It can't emit portable markers.** It has no per-column type mapping; its
   only knobs are a post-generation patch file and import lists. None of them map
   "native `uuid`/`bytea`/`timestamptz`/`jsonb` (PG) *and* native `BLOB`/`TEXT`
   (SQLite) → one `diesel_dualdb::sql_types::*` marker."

So introspecting a database is the wrong direction here.

## The approach: a single logical schema

The source of truth is the migrations, written in a **logical DDL** — ordinary
`CREATE TABLE`/`ALTER TABLE` with logical column types (`UUID`, `TIMESTAMP`,
`JSON`, …) instead of a backend's spelling. From that one source, the generator
produces:

1. **Per-backend migration trees** — concrete DDL with each backend's native
   types. These are ordinary diesel migrations, so `diesel migration run` applies
   them. (No bespoke migration runner.)
2. **`schema.rs`** — unified `table!` definitions typed with the portable
   markers.

One definition yields both the DDL you *run* and the Rust schema you *query
against*, and they can't drift. This is the same one-source-of-truth spirit as
the rest of the crate, applied to schema.

## How the DDL is rendered

Rather than re-rendering DDL from a hand-built model (which would have to model
every SQL feature to preserve it), the generator **rewrites only the column types
in the parsed statement and re-emits it** via `sqlparser`'s `Display`. So foreign
keys, `DEFAULT`, `UNIQUE`, `CHECK`, composite primary keys, `CREATE INDEX`, and
`ALTER TABLE` all pass through unchanged — only the types are swapped per
backend. A separate pass builds the model that drives `schema.rs` (markers,
nullability, the PK tuple, the foreign-key graph → `joinable!`).

## The escape hatch

A logical schema can't express genuinely backend-specific DDL (a GIN index, a
partial index, `WITHOUT ROWID`). Those go in a tagged block that's emitted
verbatim to one backend and ignored by the schema model — see
[Diverge per backend](../how-to/diverge-per-backend.md). It's the pressure valve
that keeps the common case clean without pretending every schema is fully
portable.

## Relationship to the native-type shim

A different idea — bridging diesel's *native* `sql_types` onto `MultiBackend`
(the "compatibility shim") — would let a Postgres `print-schema` run on
`MultiBackend` directly. It's a real option for "drop-in with no schema edits,"
but it's a separate, larger lever and makes Postgres the source of truth. The
logical-schema approach was chosen instead because it owns the portable mapping
explicitly, generates the SQLite migrations too, and doesn't depend on the shim.
