---
id: m1-diesel-dualdb-macros-bridge
level: initiative
title: "M1: diesel-dualdb-macros — bridge codegen + test harness"
short_code: "DDB-I-0003"
created_at: 2026-06-07T14:57:23.503211+00:00
updated_at: 2026-06-09T01:52:05.553778+00:00
parent: DDB-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/completed"


exit_criteria_met: false
estimated_complexity: M
initiative_id: m1-diesel-dualdb-macros-bridge
---

# M1: diesel-dualdb-macros — bridge codegen + test harness

> **Repurposed 2026-06-08.** M1's original charter — "implement the `MultiBackend` bridge for all portable types" — was **completed inside M0** ([[DDB-I-0002]]). The spike ([[DDB-I-0001]]) proved the bridge is a trivial 3-line-per-type pattern, so each M0 type shipped as a vertical slice (concrete + bridge + tests). Uuid/Bytes/Timestamp/Json bridges already ship and are green (20 tests). What remains is the *codegen* the design always anticipated.

## Context

DESIGN.md §6 always reserved `diesel-dualdb-macros` for "later codegen". With four types now bridged by hand (`src/backend.rs` has a `#[cfg] mod <t> { … }` block each, and every `tests/<t>.rs` repeats a 4-way macro), the boilerplate is now well understood and ripe to generate. This initiative builds the proc-macro crate that (a) generates a type's `MultiBackend` bridge from an annotation, and (b) provides `#[dualdb::test(pg, sqlite)]` — pulled forward from M2, because it's a proc-macro and belongs with the macro crate. Goal: adding a portable type or a both-backend test becomes one annotation.

## Goals & Non-Goals

**Goals:**
- New `diesel-dualdb-macros` proc-macro crate (workspace member of `diesel-dualdb`).
- **Bridge codegen macro** — generates the three `MultiBackend` impls for a portable type, replacing the hand-written `mod <t>` blocks in `src/backend.rs`. Must handle feature-gating, generic newtypes (Json's `T`), and the `Send + Sync + 'static` bound the MultiBackend `ToSql` needs (Json finding, DDB-T-0010).
- **`#[dualdb::test(pg, sqlite)]`** — expands one test body into `name_sqlite()` (in-memory) + `name_pg()` (env-gated, self-skips without `DUALDB_PG_URL`), each handed `&mut DualConnection`. Replaces the per-file 4-way test macros.
- Migrate the four existing types + their tests onto the macros; the current 20 tests stay green via `angreal test all`, and `angreal check all` stays clean.

**Non-Goals:**
- Runtime ergonomics — `Pool::connect`/URL detection, returning/escape helpers (M2, [[DDB-I-0004]]).
- New portable types (decimal/array/enum still deferred).
- Async (v2).

## Detailed Design

Two macros in `diesel-dualdb-macros`:

1. **Bridge codegen.** Expansion target = the existing `src/backend.rs` `mod {uuid,bytes,timestamp,json}` bodies (`HasSqlType` via `MultiBackend::lookup_sql_type::<ST>`, `ToSql` via `out.set_value((ST, self))`, `FromSql` via `bytes.from_sql::<Self, ST>()`). Open API choice (decide in decomposition): a `#[derive(DualBridge)]` on the newtype that reads the marker from `#[diesel(sql_type = …)]`, vs a function-like `dualdb::bridge!(Marker => Newtype)`. Derive is cleanest if it can see the sql_type attr. Must accept optional generics/bounds for Json.

2. **`#[dualdb::test(pg, sqlite)]`.** Generates the two `#[test]` fns; SQLite in-memory, Pg from `DUALDB_PG_URL` with graceful skip. Optional setup hook (DDL/migrations) so type tests can create their table. Target = the `tests/*.rs` 4-way macro bodies.

## Alternatives Considered

- **Keep bridges hand-written (ship only `#[dualdb::test]`):** genuinely viable — each bridge is ~12 lines and types are few. The macro's main payoff is the test ergonomic + future types. If the bridge-codegen proc-macro proves more complexity than it saves (generics + feature-gates are fiddly in a derive), reduce M1 to just `#[dualdb::test]` and leave bridges by hand. Decide during decomposition.
- **Put `#[dualdb::test]` in M2:** rejected — it's a proc-macro; keep all macros in one crate, leave M2 pure-runtime.

## Implementation Plan

Decompose after this repurpose is approved. Expected tasks:

1. Scaffold the `diesel-dualdb-macros` workspace crate.
2. `#[dualdb::test(pg, sqlite)]` (highest ergonomic payoff) + migrate one `tests/<t>.rs` as proof.
3. Bridge-codegen macro + migrate `Uuid` as the reference; reconcile feature-gating + generics.
4. Migrate remaining types/tests; confirm the 20 tests stay green and `angreal check all` clean.

**Exit criteria:** adding a portable type needs no hand-written `backend.rs` block; both-backend tests are written with `#[dualdb::test]`; the existing suite passes unchanged; feature matrix + clippy clean. (If decomposition concludes the bridge macro isn't worth it, M1 reduces to `#[dualdb::test]` and the rest folds into M2.)