---
id: extended-portable-types-decimal
level: initiative
title: "Extended portable types (decimal, arrays, enums)"
short_code: "DDB-I-0006"
created_at: 2026-06-10T01:55:35.510605+00:00
updated_at: 2026-06-10T02:48:26.367675+00:00
parent: DDB-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/completed"


exit_criteria_met: false
estimated_complexity: M
initiative_id: extended-portable-types-decimal
---

# Extended portable types (decimal, arrays, enums) Initiative

## Context

The v1 type set is Uuid/Bytes/Timestamp/Json. Three more were deferred at design time: **decimal**, **Postgres arrays**, and **enums**. The user pulled them into v1 (2026-06-09). Each follows the proven portable-type pattern (marker in `src/sql_types.rs`, newtype in `src/types/`, bridge in `src/backend.rs` via `bridge!` or hand-written, test in `tests/`), but the SQLite representations differ in difficulty.

Sibling decisions (native-type shim, unified `ON CONFLICT`) are **out of scope** here — both are blocked in-crate (orphan rule / derive-controlled `SqlDialect`) and will be revisited as possible upstream-diesel contributions.

## Goals & Non-Goals

**Goals:**
- **Decimal** — exact numeric across both backends (PG `numeric` ↔ SQLite TEXT), `bigdecimal::BigDecimal`.
- **Array** — Postgres arrays portably (PG native `T[]` ↔ SQLite JSON TEXT), at least for the common scalar element types.
- **Enum** — a portable enum (PG native `CREATE TYPE … AS ENUM` ↔ SQLite `TEXT` + `CHECK`), the messiest type.
- Each: marker + newtype + MultiBackend bridge + a `#[diesel_dualdb::test]` round-trip on both backends; feature-gated; `schema-gen` `map_type` row where it makes sense.

**Non-Goals:**
- The native-type shim and unified `ON CONFLICT` (blocked; separate upstream effort).
- SQLite-side arithmetic/ordering guarantees for Decimal/Array (value storage, not query-time math — documented).
- MySQL.

## Detailed Design

- **Decimal:** PG arm delegates to diesel's `Numeric`/`BigDecimal` (`diesel/numeric`). SQLite arm stores the canonical decimal **string** as TEXT (exact round-trip). Document that SQLite-side ordering/arithmetic on the TEXT isn't reliable — it's a value store. Feature `decimal` (+ `bigdecimal` dep).
- **Array:** PG arm delegates to diesel's `Array<ST>` ↔ `Vec<T>`. SQLite arm serializes `Vec<T>` to JSON TEXT (reuse the `serde_json` machinery). Likely needs the same split bounds as `Json<T>` for the MultiBackend impl; may scope to scalar element types first (Integer/Text/Uuid). Feature `array` (depends on `serde_json`).
- **Enum:** the hard one. PG needs a real `CREATE TYPE` enum + `ToSql/FromSql` mapping a Rust enum's variants ↔ labels; SQLite stores the label as TEXT (optionally a `CHECK`). Diesel has no built-in enum support (ecosystem uses `diesel-derive-enum`). Likely a derive or macro in `diesel-dualdb-macros` that emits both arms + the bridge. **May warrant its own design pass** before building — flagged in its task.

## Alternatives Considered

- **Decimal as REAL on SQLite:** rejected — lossy. TEXT is exact.
- **Decimal as scaled INTEGER:** rejected for v1 — needs a fixed scale per column (schema knowledge); TEXT is simpler and exact. Recorded as a future option if SQLite-side math is needed.
- **Array via a bespoke wire format on SQLite:** rejected — JSON TEXT reuses existing machinery and is debuggable (consistent with the Timestamp-as-TEXT rationale).
- **Skip enums:** tempting (messiest), but the user wants the full set; build it, but design-gate it.

## Implementation Plan

Decomposed 2026-06-09. Build in increasing-difficulty order:

1. **[[DDB-T-0024]] Decimal** — cleanest; mirrors the Timestamp slice (native PG / TEXT SQLite).
2. **[[DDB-T-0025]] Array** — generic element type + JSON-on-SQLite; scope to common scalars first.
3. **[[DDB-T-0026]] Enum** — design pass first, then a derive/macro emitting both arms + bridge.

**Exit criteria:** Decimal + Array + Enum each round-trip on both backends via `#[diesel_dualdb::test]`, feature-gated, documented (reference/portable-types + how-to/add-a-portable-type), `angreal check all` + `test all` green. CHANGELOG updated.