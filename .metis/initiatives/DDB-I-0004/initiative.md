---
id: m2-ergonomics-pool-test-harness
level: initiative
title: "M2: Ergonomics — pool, test harness, helpers"
short_code: "DDB-I-0004"
created_at: 2026-06-07T14:57:24.937808+00:00
updated_at: 2026-06-10T01:45:42.687768+00:00
parent: DDB-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/completed"


exit_criteria_met: false
estimated_complexity: M
initiative_id: m2-ergonomics-pool-test-harness
---

# M2: Ergonomics — pool, test harness, helpers Initiative

## Context

M2 (DESIGN.md §4–5, roadmap M2) turns the proven core into something pleasant to adopt. With the bridge working (DDB-I-0003), queries already run; this milestone removes the remaining friction: connecting/pooling in one line, writing both-backend tests with one attribute, and making insert-RETURNING / upsert / the rare per-backend escape ergonomic. After M2, v1 is shippable.

## Goals & Non-Goals

**Goals:**
- **`Pool` + URL detection** (`src/pool.rs`): `dualdb::Pool::connect(&url)` inspects scheme/path (`postgres://`/`postgresql://` → Pg; `file:`/path/`:memory:` → SQLite) and returns an r2d2 pool yielding `DualConnection`. `pool.get()` → pooled `DualConnection`.
- **`#[dualdb::test(pg, sqlite)]`** proc-macro (`diesel-dualdb-macros/`): expands a single test body into one test per named backend, each handed a `&mut DualConnection`; skips Pg cleanly when `DUALDB_PG_URL` is unset. Replaces the M0 primitive runner.
- **Returning/upsert helpers** (`src/returning.rs`): smooth the insert-RETURNING ergonomics and paper over upsert feature-gap differences where it's safe to do so generically.
- **Escape-hatch helpers** (`src/escape.rs`): optional sugar for the rare `match conn { Pg(c) => …, Sqlite(c) => … }` path — make divergence loud and tidy, never hidden.
- Docs: README usage examples (the DESIGN.md §5 snippets), per-module rustdoc, a "when to use the escape hatch" guide.

**Non-Goals:**
- Async (`AsyncDualConnection`) — v2 / DDB roadmap M3, separate initiative when v1 ships.
- Native-type compatibility shim, decimal/array/enum types, migration tooling — deferred.
- Macro-generating the bridge impls (M1 chose hand-written; revisit only if the type set grows).

## Detailed Design

- **URL detection** is a pure function over the connection string, unit-testable without a live DB; the pool wrapper is thin over `r2d2::Pool<ConnectionManager<DualConnection>>` (manager path confirmed in M1).
- **`#[dualdb::test]`** is the first real consumer of `diesel-dualdb-macros`. It generates `#[test] fn name_pg()` / `fn name_sqlite()`, each setting up a connection (Pg from env, SQLite in-memory), running migrations/DDL hook if provided, invoking the body. Arm-agreement is structural: same body, two backends, both must pass.
- **Returning/upsert**: lean on what M1 proved works on one arm; only add per-backend branches where upsert genuinely diverges (`ON CONFLICT` feature gaps), and surface those loudly.
- **Escape helpers**: a thin typed wrapper so the two-arm match reads cleanly and the compiler enforces both arms are handled.

## Alternatives Considered

- **Skip the macro, ship only the primitive runner from M0:** rejected — the one-attribute both-backend test is a headline ergonomic and the arm-agreement enforcement mechanism; worth the proc-macro crate.
- **Generic upsert abstraction across backends:** mostly rejected — upsert divergence is real (the escape-hatch case); abstract only the safe common subset, keep the rest explicit per principle 1.
- **Bundle async into v1:** rejected — locked decision (sync first); async has no `MultiConnection` equivalent and is the largest single build item (v2).

## Implementation Plan

> **Already shipped in M1 (DDB-I-0003):** the `diesel-dualdb-macros` crate, `#[diesel_dualdb::test(pg, sqlite)]`, and the test migration onto it (original plan steps 1 & 3). The schema generator (DDB-I-0005) also shipped. So M2 is the remaining *runtime ergonomics + release polish*.

Decomposed 2026-06-09. Remaining tasks:

1. **[[DDB-T-0020]] `Pool::connect` + URL/scheme detection** — `dualdb::Pool::connect(&url)` inspects the URL (`postgres://`/`postgresql://` → Pg; `file:`/path/`:memory:` → SQLite) and returns an r2d2 pool of `DualConnection`. Fixes the spike's finding that the derived `establish` naively tries Pg-then-Sqlite. URL detection is a pure, unit-tested function.
2. **[[DDB-T-0021]] Returning / upsert helpers** — smooth insert-`RETURNING`; provide a portable upsert (`ON CONFLICT`) helper for the safe common subset, leaving genuine divergence to the escape hatch.
3. **[[DDB-T-0022]] Escape-hatch helpers + "when to diverge" docs** — a thin typed wrapper so the rare two-arm `match conn` reads cleanly and the compiler enforces both arms.
4. **[[DDB-T-0023]] v1 release polish** — rustdoc pass, crate metadata (`repository`/`homepage`), README, MSRV/feature notes, CHANGELOG + a release checklist. Closes v1 (sync).

**Exit criteria:** `Pool::connect` picks the right backend from a URL and yields working pooled connections; returning/escape helpers exist and are documented with examples; the crate is publishable as v1 (sync).