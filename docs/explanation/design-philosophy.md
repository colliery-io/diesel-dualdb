# Explanation: design philosophy

## The problem

Projects that target both PostgreSQL and SQLite pay three boilerplate taxes:

1. **Type mapping.** Postgres has `uuid`, `timestamptz`, `bool`, `jsonb`,
   arrays, enums; SQLite has TEXT/INTEGER/REAL/BLOB/NULL. Every project
   re-hand-writes `ToSql`/`FromSql` pairs for the same logical types.
2. **Connection/pool dispatch.** Wrapping connections, detecting the backend,
   routing.
3. **Query divergence.** Some SQL genuinely differs per backend; most does not —
   but tooling often forces you to write even the identical queries twice.

diesel-dualdb kills (1), makes (2) trivial, and collapses (3) to one arm except
where divergence is real.

## What Diesel already gives us, and the gap

`#[derive(diesel::MultiConnection)]` (2.1+) generates an enum connection and a
unified `MultiBackend`, so a single function can compile against both backends.
That's the foundation. But the generated `MultiBackend` implements `HasSqlType`
only for Diesel's common required set (the small/int/bigint, float/double, text,
binary, date, time, timestamp, bool group). Anything outside it — notably
`Uuid` — has no `HasSqlType`, so `QueryMetadata` is missing, so `get_result` /
`RETURNING` fail ([diesel-rs/diesel#4079](https://github.com/diesel-rs/diesel/issues/4079)).

That single hole is what pushes projects off the unified path and into pulling
concrete connections and duplicating every query. The derive documents the fix —
provide `HasSqlType`/`FromSql`/`ToSql` for the type against each inner backend
*and* against `MultiBackend` — and that's exactly the extension point this crate
occupies. (See [Architecture](architecture.md).)

## One arm by default; two arms only when forced; never a hidden third path

The failure mode we explicitly reject: a "unified" layer that is partially
bypassed, so any query might live in one of three places — shared, pg-override,
sqlite-override — and you can't tell which without reading all three.

The goal isn't DRY over locality. It's to make the one-arm path **reliable**, so
locality and DRY stop being in tension: when write-once genuinely compiles and
runs on both backends, "where does this query resolve?" has one answer almost
always, and the rare escape is loud and obvious.

Concretely:

- The default shape of a query is a single function taking `&mut DualConnection`.
- The escape hatch is a `match` on the connection enum — exhaustive, visible at
  the call site — used only for SQL that genuinely can't be shared. See
  [Diverge per backend](../how-to/diverge-per-backend.md).
- Agreement between the arms is enforced by running the same behavioural suite
  against both backends (`#[diesel_dualdb::test]`), not by code unification.

## Scope honesty

Exactly two backends: PostgreSQL and SQLite. "Dual" is literal. Arbitrary-N would
dilute the value and the testing story; MySQL is resisted.

## Roadmap

- **v1 (sync):** portable types, the `MultiBackend` bridge, the test and bridge
  macros, the schema generator, `Pool::connect` with URL/scheme detection, and
  the `dispatch` escape hatch.
- **v2:** an async `DualConnection` (there is no `MultiConnection` equivalent in
  `diesel-async`, so it must be hand-built — Postgres native, SQLite via
  `SyncConnectionWrapper`).
- **Won't-fix in-crate (see ADR DDB-A-0002):** a *native-type compatibility
  shim* (use `diesel::sql_types::Uuid` etc. directly) is impossible — the orphan
  rule blocks the SQLite impls and diesel doesn't support those PG-native types
  on SQLite. The portable types are the answer. A *unified `ON CONFLICT`* through
  `MultiBackend` is blocked by the `MultiConnection` derive's fixed dialect;
  upsert stays an escape-hatch (`dispatch`) case, with the real fix being an
  upstream diesel change.
- **Out of scope:** MySQL.
