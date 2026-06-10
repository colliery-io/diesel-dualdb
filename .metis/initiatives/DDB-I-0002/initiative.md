---
id: m0-portable-type-layer
level: initiative
title: "M0: Portable type layer"
short_code: "DDB-I-0002"
created_at: 2026-06-07T14:57:22.992221+00:00
updated_at: 2026-06-09T01:04:54.801338+00:00
parent: DDB-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/completed"


exit_criteria_met: false
estimated_complexity: M
initiative_id: m0-portable-type-layer
---

# M0: Portable type layer Initiative

## Context

Layer A of the architecture (DESIGN.md §4): portable marker SQL types + domain newtypes with per-backend `ToSql`/`FromSql`. Defined against `Backend`, not connections, so this layer is identical for sync and async and has standalone value even without the bridge. Starts after the spike (DDB-I-0001) validates conventions; the spike's Uuid implementation is promoted from throwaway to production-quality here.

**Spike inputs (DDB-I-0001, GO):** conventions, the `returning_clauses_for_sqlite_3_35` required-feature, the diesel 2.3 pin and the bridge-is-trivial finding all come from the spike report. Uuid is already implemented end-to-end (concrete + bridge + tests).

| Logical | Postgres | SQLite | Rust wrapper | Notes |
|---|---|---|---|---|
| UUID | `uuid` | BLOB (16 bytes) | `types::Uuid(uuid::Uuid)` | ✅ done in spike (concrete + bridge); M0 only feature-gates it |
| Timestamp | `timestamptz` | TEXT (RFC3339, fixed format) | `types::Timestamp(DateTime<Utc>)` | format locked by ADR, see below |
| Binary | `bytea` | BLOB | `types::Bytes(Vec<u8>)` | |
| Json | `jsonb` | TEXT | `types::Json<T>(T)` | serde round-trip; Pg jsonb needs 0x01 version-byte prefix; SQLite 3.45 JSONB deferred |

**~~Bool~~ dropped:** spike confirmed `MultiBackend` already implements `HasSqlType<Bool>` (native required core set). No `types::Bool` needed.

**Decomposition choice (carried into tasks):** since the spike proved the `MultiBackend` bridge is a trivial 3-line-per-type pattern, each remaining type is built as a **vertical slice** — concrete `Pg`/`Sqlite` impls **+ the bridge impl + both the round-trip and one-arm ORM tests** — in one task, rather than splitting concrete (M0) from bridge (M1) and passing over each type twice. This pulls M1's per-type bridge work into M0; **M1 (DDB-I-0003) then narrows to macro-izing the bridge + comprehensive nullable/cross-type coverage** (or merges away). Flag for human confirmation at decomposition review.

## Goals & Non-Goals

**Goals:**
- All initial types implemented with `ToSql`/`FromSql` for `Pg` and `Sqlite` (concrete backends; MultiBackend bridge is M1).
- Round-trip tests for every type, run against **both** backends — requires a primitive both-backend test runner now (full `#[dualdb::test]` macro is M2).
- CI story for Postgres decided and working from this milestone: env-gated (`DUALDB_PG_URL`, skip-if-absent) locally, real Postgres service in CI.
- **ADR: SQLite timestamp representation** — RFC3339 TEXT, locked to UTC-only, `Z` suffix, fixed fractional precision (lexicographic sort must equal chronological sort); document rejected alternative (unix INTEGER).
- Feature flags per Diesel convention: `uuid`, `chrono`, `serde_json` optional deps; Diesel version range + MSRV pinned.

**Non-Goals:**
- MultiBackend bridge impls (M1).
- Deferred types: decimal, Pg arrays, portable enums.
- Pool, macros, ergonomics (M2). Async (v2).

## Detailed Design

Module layout per DESIGN.md §6: `src/sql_types.rs` (markers), `src/types/{uuid,timestamp,bool,bytes,json}.rs` (newtypes + impls), `tests/types.rs` + `tests/harness.rs` (parameterized runner primitive).

Per-type conventions established by the spike apply: marker derives `SqlType` + `QueryId` with `postgres_type`/`sqlite_type` annotations; newtype derives `AsExpression` + `FromSqlRow` with `#[diesel(sql_type = ...)]`. Pg impls delegate to diesel's native support where it exists (uuid, chrono, serde_json features); SQLite impls hand-encode (BLOB for uuid/bytes, formatted TEXT for timestamp, serialized TEXT for json).

Edge cases the round-trip suite must cover: nullable columns (`Option<T>`), empty bytes vs NULL, timestamp precision truncation, sub-second ordering, json with non-ASCII and nested structures, uuid nil/max values.

## Alternatives Considered

- **TEXT (hyphenated) UUIDs on SQLite:** rejected in design seed — 36 bytes vs 16, index bloat; debuggability cost accepted.
- **Unix-integer timestamps on SQLite:** rejected for human readability; revisit only if range-query performance demands it (record in the ADR).
- **One mega-module for types:** rejected; per-type files keep impls reviewable and feature-gateable.

## Implementation Plan

Decomposed 2026-06-08 (post-spike). Tasks:

1. **Crate hygiene & feature flags** ([[DDB-T-0006]]) — optional `uuid`/`chrono`/`serde_json` deps behind features; `returning_clauses_for_sqlite_3_35` as a **required** diesel feature; verify the real MSRV (1.84 is provisional); `cargo check` per-feature and `--no-default-features`; feature-gate the existing Uuid type.
2. **CI workflow** ([[DDB-T-0007]]) — GitHub Actions running `angreal test all` with a Postgres service + SQLite (the local orchestration from [[DDB-T-0005]] already exists; this wires it to CI).
3. **ADR: SQLite timestamp representation** ([[DDB-A-0001]]) — RFC3339 TEXT, UTC-only, `Z` suffix, fixed fractional precision; rejected alt = unix INTEGER. Blocks the Timestamp task.
4. **Timestamp type** ([[DDB-T-0008]]) — vertical slice (concrete + bridge + round-trip + ORM test). Depends on the ADR.
5. **Bytes type** ([[DDB-T-0009]]) — vertical slice; empty-bytes-vs-NULL edge case.
6. **Json\<T\> type** ([[DDB-T-0010]]) — vertical slice; Pg `jsonb` 0x01 version-byte prefix; non-ASCII + nested coverage.

Uuid is already done (spike); not a task here beyond feature-gating in #1. Bool dropped.

**Exit criteria:** every shipped type round-trips identically AND passes a one-arm ORM test on both backends in CI; timestamp ADR published; feature matrix builds (`cargo check` per-feature and `--no-default-features`); MSRV confirmed.