---
id: diesel-dualdb
level: vision
title: "Diesel-DualDB"
short_code: "DDB-V-0001"
created_at: 2026-06-07T14:49:51.390394+00:00
updated_at: 2026-06-07T14:49:51.390394+00:00
archived: false

tags:
  - "#vision"
  - "#phase/draft"


exit_criteria_met: false
initiative_id: NULL
---

# Diesel-DualDB Vision

## Purpose

`diesel-dualdb` is a Diesel companion crate that lets projects write query code **once** and run it against both PostgreSQL and SQLite, collapsing dual-backend boilerplate to a single arm and keeping per-backend divergence as a rare, explicit exception.

Projects repeatedly targeting both backends pay three boilerplate taxes today: (1) hand-written `ToSql`/`FromSql` pairs for the same logical types (UUID, timestamps, JSON, bools), (2) connection/pool dispatch plumbing, and (3) duplicated queries even when the SQL is identical. This crate kills (1) outright, makes (2) trivial, and collapses (3) to one arm except where divergence is real.

## Product/Solution Overview

A library crate built on Diesel's `#[derive(MultiConnection)]` (2.1+). The derive already gives a unified enum connection and `MultiBackend`, but the generated backend only implements `HasSqlType` for Diesel's required core set — anything beyond (notably `Uuid`) lacks `QueryMetadata`, so `get_result`/`RETURNING` fail (diesel-rs/diesel#4079). That single hole forces projects off the unified path into wholesale query duplication (see prior art: `colliery-io/cloacina`, 119 dispatch sites, many byte-identical between arms).

The crate occupies exactly the extension point the derive documents: provide `HasSqlType`, `FromSql`, and `ToSql` for portable types against each inner backend **and** the generated `MultiBackend`.

Two layers:
- **Layer A — portable types** (runtime-agnostic): marker SQL types (`dualdb::sql_types::*`) + domain newtypes (`dualdb::types::*`) with per-backend impls. Initial set: Uuid (Pg `uuid` / SQLite 16-byte BLOB), Timestamp (Pg `timestamptz` / SQLite RFC3339 TEXT), Bool, Bytes, Json (Pg `jsonb` / SQLite TEXT).
- **Layer B — connection/execution bridge** (sync): `DualConnection` enum via `MultiConnection`, plus the bridge trait impls on `MultiBackend` that make `get_result`/`RETURNING` work on one arm.

## Current State

Design seed complete (see `DESIGN.md` in repo root). No code yet. Reference implementation of the *problem* (not the solution) lives in `../cloacina` — `src/database/universal_types.rs`, `src/database/connection/backend.rs`, `src/dal/unified/` show both the hand-rolled type impls to crib from and the duplication this crate eliminates.

## Future State

- The default shape of a query is a single function taking `&mut DualConnection`; `get_result`/`RETURNING` work on one arm for all portable types.
- The escape hatch is a `match` on the connection enum — rare, loud, explicit — for SQL that genuinely cannot be shared (locking hints, CTE+DML-RETURNING, FTS/JSON-operator divergence).
- Arm agreement enforced by a test harness running the same behavioral suite against both backends (`#[dualdb::test(pg, sqlite)]`).
- Pooling and backend detection are one-liners: `dualdb::Pool::connect(&url)` → `DualConnection`.
- Async (`AsyncDualConnection`) follows as v2; there is no `MultiConnection` equivalent in `diesel-async`, so it must be hand-built (Pg native, SQLite via `SyncConnectionWrapper`).

## Major Features

- **Portable type layer**: write-once `ToSql`/`FromSql` for Uuid, Timestamp, Bool, Bytes, Json across Pg, SQLite, and MultiBackend.
- **The bridge**: closes #4079 locally — `HasSqlType` on `MultiBackend` so `QueryMetadata` is satisfied and one-arm `get_result`/`RETURNING` is reliable.
- **Pool + URL detection**: scheme/path detection picks the backend; r2d2-based pooling over the enum connection.
- **Both-backends test harness**: `#[dualdb::test(pg, sqlite)]` runs the body once per backend; arm agreement via tests, not code unification.

## Success Criteria

- A downstream project can express its common queries as single functions over `&mut DualConnection`, including insert-with-RETURNING, with zero per-backend duplication.
- Round-trip tests for every portable type pass identically on both backends.
- Adoption is a schema column-type swap (`Uuid` → `dualdb::sql_types::Uuid`), nothing more.
- cloacina (or an equivalent) could migrate and delete its unified-DAL dispatch layer.

## Principles

1. **One arm by default; two arms only when the SQL truly cannot be shared; never a hidden third path.** Any given query resolves in exactly one place.
2. **Build on Diesel's extension points, don't reinvent.** `MultiConnection` is the foundation; the crate fills its documented gap.
3. **Spike the riskiest unknown first.** The `FromSql<_, MultiBackend>` plumbing and pooling derive are proven on `Uuid` before anything else is built.
4. **Arm agreement via tests, not code unification.**
5. **Scope honesty.** Exactly two backends — "dual" is literal. Resist MySQL.

## Constraints

(Locked decisions from the design seed)

1. **Name:** `diesel-dualdb` — literal and discoverable.
2. **Scope:** Postgres + SQLite only, not arbitrary N.
3. **Order:** sync first (on `MultiConnection`); async is v2 fast-follow.
4. **Drop-in target:** adopt via schema column-type swap; a native-type compatibility shim (no schema edits at all) is evaluated only after the spike.
5. **Migrations out of v1:** separate per-backend migration trees (the cloacina pattern).
6. **Deferred types:** numeric/decimal, Pg arrays, portable enums (messiest; designed separately).

### Open risks (from design review, 2026-06-07)

- **Bool may already be supported** by `MultiBackend`'s required core set — verify in spike; if so, drop `types::Bool` from scope.
- **Timestamp TEXT sorting**: RFC3339 only sorts lexicographically with a fixed format — must lock UTC-only, `Z` suffix, fixed fractional precision via ADR before M0 freezes.
- **CI story for Postgres** (env-gated vs containers) is needed by M0, not M2 — round-trip tests already require both backends.
- **Feature flags**: `uuid`, `chrono`, `serde_json` as optional deps per Diesel convention; pin Diesel version range + MSRV at project start.
- **Pg `jsonb` wire format**: 0x01 version-byte prefix on ToSql/FromSql.