# diesel-dualdb

A Diesel companion crate that lets you write query code **once** and run it against
both PostgreSQL and SQLite, collapsing dual-backend boilerplate to a single arm and
keeping per-backend divergence as a rare, explicit exception.

Status: design seed. Sync first; async is the planned fast-follow.

---

## 1. Problem

We repeatedly write backends that target SQLite and Postgres interchangeably. Today that
costs three kinds of boilerplate:

1. **Type mapping.** Postgres has native `uuid`, `timestamptz`, `bool`, `jsonb`, arrays,
   enums; SQLite has TEXT/INTEGER/REAL/BLOB/NULL. Every project re-hand-writes `ToSql`/`FromSql`
   pairs for the same logical types.
2. **Connection / pool dispatch.** Wrapping connections and pools, detecting the backend,
   routing.
3. **Query divergence.** Some SQL genuinely differs per backend; most does not, but the
   tooling often forces you to write even the identical queries twice.

This crate exists to kill (1) outright, make (2) trivial, and make (3) collapse to one arm
except where divergence is real.

## 2. What Diesel already gives us, and the gap

- `#[derive(diesel::MultiConnection)]` (2.1+) generates an enum connection and a unified
  `MultiBackend`, so a single function can compile against both backends. This is the
  foundation of the one-arm approach; we build on it, we do not reinvent it.
- **The gap (issue #4079):** the generated `MultiBackend` implements `HasSqlType` only for
  Diesel's common required set (small/int/bigint, float/double, text, binary, date, time,
  timestamp). Anything outside that set, notably `Uuid`, has no `HasSqlType`, so
  `QueryMetadata` is missing, so `get_result` / `RETURNING` fails. That single hole is what
  forces projects (see cloacina) off the unified path and into pulling concrete connections
  and duplicating every query.
- The derive documents the fix: provide `HasSqlType`, `FromSql`, and `ToSql` for the type
  against each inner backend **and** against the generated `MultiBackend`. That is precisely
  the extension point this crate occupies.
- **Async:** there is no `MultiConnection` equivalent in `diesel-async`. Postgres is natively
  async; SQLite runs through `SyncConnectionWrapper`. A unified async connection must be
  hand-built. This is deferred to v2 and is the single largest build item.

## 3. Design philosophy

**One arm by default; two arms only when the SQL truly cannot be shared; never a hidden
third path.**

The failure mode we are explicitly rejecting: a "unified" layer that is partially bypassed,
so any given query might live in one of three places (shared, pg-override, sqlite-override)
and you cannot know which without reading all three. cloacina lives in this state today, not
by choice but because #4079 forced concrete connections; the result was 119 dispatch sites,
many of them byte-identical between arms.

We are not choosing DRY over locality. We are making the one-arm path **reliable** so that
locality and DRY stop being in tension: when write-once genuinely compiles and runs on both
backends, "where does this query resolve" has one answer almost always, and the rare escape
is loud and obvious.

Concretely:

- The default shape of a query is a single function taking `&mut DualConnection`.
- The escape hatch is a `match` on the connection enum, used only for SQL that cannot be
  expressed identically (locking hints, CTE + DML-`RETURNING`, recursive/FTS/JSON-operator
  divergence, upsert feature gaps).
- Arm agreement for the rare escapes is enforced by a test harness that runs the same
  behavioral suite against both backends, not by code unification.

## 4. Architecture

Two layers with different runtime coupling.

### Layer A: portable types (runtime-agnostic core)

`ToSql`/`FromSql` are defined against a `Backend`, not a connection, so this layer is
identical for sync and async and is the reusable heart of the crate. It is also the piece
that, by being bridged onto `MultiBackend`, closes #4079.

Two pieces per logical type:

- a **marker SQL type** (`dualdb::sql_types::*`) annotated with its native representation per
  backend;
- a **domain newtype** (`dualdb::types::*`) wrapping the Rust value, with `ToSql`/`FromSql`
  for `Pg`, for `Sqlite`, and for `MultiBackend`.

Initial type set:

| Logical    | Postgres      | SQLite              | Rust wrapper            | Notes |
|------------|---------------|---------------------|-------------------------|-------|
| UUID       | `uuid`        | `BLOB` (16 bytes)   | `types::Uuid(uuid::Uuid)` | BLOB, not TEXT, for index size |
| Timestamp  | `timestamptz` | `TEXT` (RFC3339)    | `types::Timestamp(DateTime<Utc>)` | RFC3339 chosen for human-readable, sortable; revisit vs unix int |
| Bool       | `bool`        | `INTEGER` (0/1)     | `types::Bool(bool)`     | |
| Binary     | `bytea`       | `BLOB`              | `types::Bytes(Vec<u8>)` | |
| Json       | `jsonb`       | `TEXT`              | `types::Json<T>(T)`     | serde round-trip; SQLite 3.45 JSONB deferred |

Deferred types: numeric/decimal, Postgres arrays (as JSON on SQLite), portable enums
(Postgres native enum vs SQLite TEXT + CHECK).

Marker + wrapper sketch:

```rust
// dualdb::sql_types
#[derive(diesel::sql_types::SqlType, diesel::query_builder::QueryId, Clone, Copy, Debug)]
#[diesel(postgres_type(name = "uuid"))]
#[diesel(sqlite_type(name = "Binary"))]
pub struct Uuid;

// dualdb::types
#[derive(AsExpression, FromSqlRow, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[diesel(sql_type = crate::sql_types::Uuid)]
pub struct Uuid(pub uuid::Uuid);

impl ToSql<sql_types::Uuid, Pg>     for types::Uuid { /* delegate to uuid native */ }
impl FromSql<sql_types::Uuid, Pg>   for types::Uuid { /* delegate to uuid native */ }
impl ToSql<sql_types::Uuid, Sqlite> for types::Uuid { /* 16-byte BLOB */ }
impl FromSql<sql_types::Uuid, Sqlite> for types::Uuid { /* BLOB -> Uuid */ }
```

### Layer B: connection / execution (the bridge, sync)

The crate owns the canonical connection enum, so it also owns `MultiBackend` and can
implement the bridge traits locally (no orphan problem):

```rust
#[derive(diesel::MultiConnection)]
pub enum DualConnection {
    Pg(diesel::PgConnection),
    Sqlite(diesel::SqliteConnection),
}
```

The bridge: for each portable SQL type, implement the three traits against the generated
backend, following Diesel's documented `MyEnum`/`MyInteger` extension example.

```rust
impl HasSqlType<sql_types::Uuid> for MultiBackend { /* dispatch metadata per inner backend */ }
impl ToSql<sql_types::Uuid, MultiBackend>   for types::Uuid { /* dispatch to inner ToSql */ }
impl FromSql<sql_types::Uuid, MultiBackend> for types::Uuid { /* match inner RawValue */ }
```

This is the milestone that makes write-once real: with `HasSqlType<Uuid>` present on
`MultiBackend`, `QueryMetadata` is satisfied and `get_result` / `RETURNING` work on one arm.

> **Spike (v1, highest risk):** the exact `FromSql<_, MultiBackend>` plumbing (matching the
> generated `MultiBackend::RawValue`) and confirming `DualConnection` derives the
> `R2D2Connection` impl needed for pooling. Prove this on `Uuid` before building the rest.

## 5. What using it looks like

**Common case, one arm:**

```rust
pub fn count_running(conn: &mut DualConnection) -> QueryResult<i64> {
    task_executions::table
        .filter(task_executions::status.eq("Running"))
        .count()
        .get_result(conn)            // works on both backends; no dispatch
}

pub fn insert_task(conn: &mut DualConnection, new: &NewTask) -> QueryResult<Task> {
    diesel::insert_into(task_executions::table)
        .values(new)
        .get_result(conn)            // RETURNING works on both, post-bridge
}
```

**Rare escape, two arms, loud and explicit:**

```rust
pub fn claim_ready(conn: &mut DualConnection, limit: i64) -> QueryResult<Vec<ClaimResult>> {
    match conn {
        DualConnection::Pg(c)     => claim_ready_pg(c, limit),     // CTE + FOR UPDATE SKIP LOCKED
        DualConnection::Sqlite(c) => claim_ready_sqlite(c, limit), // BEGIN IMMEDIATE strategy
    }
}
```

**Pooling and backend detection:**

```rust
let pool = dualdb::Pool::connect(&database_url)?;   // scheme/path detection picks the backend
let mut conn = pool.get()?;                          // -> DualConnection
```

**Testing, arm-agreement enforced:**

```rust
#[dualdb::test(pg, sqlite)]          // runs the body once per backend
fn uuid_round_trips(conn: &mut DualConnection) {
    // identical assertions; both backends must pass
}
```

## 6. Crate layout

```
diesel-dualdb/
  src/
    lib.rs
    sql_types.rs       // Db marker SQL types
    types/             // domain newtypes + per-backend ToSql/FromSql
      uuid.rs timestamp.rs bool.rs bytes.rs json.rs
    backend.rs         // DualConnection + MultiBackend bridge impls
    pool.rs            // URL detection, Pool, connection acquisition
    returning.rs       // insert-returning / upsert ergonomics
    escape.rs          // optional helpers for the rare per-backend path
  diesel-dualdb-macros/  // #[dualdb::test], later codegen
  tests/
    harness.rs         // both-backends parameterized runner
    types.rs migrations.rs
```

## 7. Locked decisions

1. **Name:** `diesel-dualdb`; literal and discoverable over mythological.
2. **Scope:** exactly two backends (Postgres + SQLite), not arbitrary N. "Dual" is honest.
3. **Order:** sync first (built on `MultiConnection`), async fast-follow.
4. **One-arm-dominant:** the unified path is the default and must be reliable; the per-backend
   `match` escape is rare and explicit; no hidden third resolution path.
5. **Drop-in target:** adopt via schema column-type swap (`Uuid -> dualdb::sql_types::Uuid`,
   etc.); see open question 1 on whether we can avoid even that.
6. **Arm agreement via tests,** not code unification.
7. **Migrations out of v1:** keep separate per-backend migration trees (the cloacina pattern).

## 8. Risks and open questions

1. **Drop-in friction (affects decision 5).** Cleanest drop-in would bridge Diesel's *native*
   `diesel::sql_types::*` onto `MultiBackend` so existing schemas need no edits. Feasible
   (backend is local), but forces a SQLite representation choice for native `Uuid`/`Timestamptz`
   and risks import shadowing. Decision: ship portable types first; evaluate a native-bridge
   compatibility shim after the spike.
2. **MultiBackend `FromSql` plumbing** (the v1 spike, section 4).
3. **Runtime SQL divergence on the one arm:** type affinity, collation, empty-string-vs-null,
   boolean coercion. The both-backends harness is mandatory, not optional.
4. **Async `AsyncConnection` for an enum** is large; SQLite via `SyncConnectionWrapper`. v2.
5. **Timestamp representation** on SQLite: RFC3339 TEXT (chosen) vs unix INTEGER. Affects range
   queries and index behavior; document and freeze early.
6. **Portable enums:** Postgres native enum vs SQLite TEXT+CHECK is the messiest type; defer
   and design separately.

## 9. Roadmap

- **M0 - Type layer.** `sql_types` + `types` + per-backend `ToSql`/`FromSql` + round-trip tests.
  Standalone value even without the bridge.
- **M1 - The bridge (the bet).** `DualConnection`, `HasSqlType`/`ToSql`/`FromSql` on
  `MultiBackend`, prove `get_result`/`RETURNING` on one arm. Validate the spike on `Uuid` first.
- **M2 - Ergonomics.** `Pool::connect` + URL detection, returning/upsert helpers, `#[dualdb::test]`
  harness, escape-hatch helpers.
- **M3 - Async (fast-follow).** `AsyncDualConnection`, Pg native + SQLite wrapper.
- **Deferred.** Native-type compatibility shim, decimal/array/enum types, migration tooling,
  possibly MySQL (would break the "dual" framing; resist).

## References

- Diesel `MultiConnection` derive (extension via `HasSqlType`/`FromSql`/`ToSql` for `MultiBackend`):
  https://docs.rs/diesel/latest/diesel/derive.MultiConnection.html
- Issue #4079, `get_result` + `QueryMetadata` on `MultiBackend`:
  https://github.com/diesel-rs/diesel/issues/4079
- Prior art (per-backend duplication forced by #4079): `colliery-io/cloacina`,
  `src/database/universal_types.rs`, `src/database/connection/backend.rs`, `src/dal/unified/`.
