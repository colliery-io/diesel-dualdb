---
id: m3-async-asyncdualconnection
level: initiative
title: "M3: Async (AsyncDualConnection)"
short_code: "DDB-I-0007"
created_at: 2026-06-10T03:11:39.317703+00:00
updated_at: 2026-06-10T03:11:39.317703+00:00
parent: DDB-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/discovery"


exit_criteria_met: false
estimated_complexity: M
initiative_id: m3-async-asyncdualconnection
---

# M3: Async (AsyncDualConnection) Initiative

> **Status: DISCOVERY — findings below; strategic direction NOT yet chosen.** Do not decompose/build until the path is picked with the user.

## Context

v1 (sync) is complete. Async was always the v2 fast-follow (DESIGN.md M3). The seed assumption: "no `MultiConnection` equivalent in `diesel-async`, so hand-build it — Pg native, SQLite via `SyncConnectionWrapper`."

## Discovery findings (2026-06-10)

**Landscape:** `diesel-async` 0.7.0, compatible with diesel 2.3. It ships `AsyncPgConnection` (native, tokio-postgres), `AsyncMysqlConnection`, and `SyncConnectionWrapper<C>` (wraps a sync `Connection` and runs it on `spawn_blocking`). There is **no async `MultiConnection`** derive.

**The crux — a unified async connection needs ONE `Backend`.** Our type layer + bridge are all keyed on `MultiBackend`, so any async story that reuses them must also be `Backend = MultiBackend`.

**Why the cheap path (`SyncConnectionWrapper<DualConnection>`) is BLOCKED:** `SyncConnectionWrapper<C>` requires `C::Backend::BindCollector: MoveableBindCollector` **and** `C::LoadConnection::Row: IntoOwnedRow` (that's why its docs say "only SQLite is supported"). The `#[derive(MultiConnection)]`-generated `MultiBackend`:
- emits only the standard `BindCollector`/`Row`, **not** `MoveableBindCollector`/`IntoOwnedRow`;
- and we **can't add them**: the generated `MultiBindCollector` lives in a *private* `bind_collector` module (not re-exported), so it isn't even nameable from our crate — and even if it were, its bind values are borrowed type-erased `&dyn Any`, which have no owned (`Send + 'static`) form to "move," which is exactly what `MoveableBindCollector::moveable()` demands.

**Consequence:** there is **no cheap, in-crate async path**. Every option below is substantial; this is the project's "largest single build item," confirmed.

## Goals & Non-Goals

**Goals:**
- An `AsyncDualConnection` (or equivalent) that runs the **same** write-once portable-type queries over both backends, async — reusing Layer A (types) and the `MultiBackend` bridge unchanged.
- Async pooling (bb8/deadpool) and an async test harness, mirroring the sync ergonomics.

**Non-Goals:**
- Re-deriving the whole sync stack. Async must reuse the existing type layer + bridge, not fork them.
- MySQL.

## Options (the strategic decision)

| Option | What | Effort | Pg perf | Ships in-crate now? |
|--------|------|--------|---------|----------------------|
| **A. Upstream-first** | Contribute `MoveableBindCollector` + `IntoOwnedRow` to diesel's `MultiConnection` derive; then `SyncConnectionWrapper<DualConnection>` "just works" and our async layer is a thin wrapper | Small *for us* (a wrapper) + an upstream PR to diesel | spawn_blocking (not native) | No — gated on upstream acceptance |
| **B. Own an async-capable MultiConnection in-crate** | Fork/rewrite the MultiConnection machinery in our macro crate to also emit `MoveableBindCollector` (with an owned/moveable bind collector we design) + `IntoOwnedRow`; then wrap with `SyncConnectionWrapper` | Large (~reimplement the derive) + ongoing maintenance | spawn_blocking | Yes |
| **C. Native-async hand-rolled enum** | `enum { Pg(AsyncPgConnection), Sqlite(SyncConnectionWrapper<SqliteConnection>) }` with a hand-written `AsyncConnection` whose `Backend = MultiBackend`, bridging MultiBackend's query machinery to each arm | Largest (async re-impl of the MultiConnection connection layer) | **native Pg** | Yes |

**Spike target (whichever path):** prove a single portable-type round-trip (e.g. `Uuid`) async on **both** backends through one connection, reusing the existing bridge. The spike's job is to de-risk the `MoveableBindCollector`/`IntoOwnedRow` (A/B) or the unified-async-`load`/`execute` glue (C).

## Recommendation (for discussion)

Lead with **A (upstream-first)**: it's by far the smallest total effort, it makes the async story essentially free in our crate, and it benefits the whole ecosystem (the same way the RETURNING feature already unblocked sync). Its costs — depends on diesel maintainers, and Pg runs on `spawn_blocking` rather than native async — are acceptable for a v2 (SQLite is blocking anyway). If native-async Pg is a hard requirement, that's **C**, and it's a major build. **B** is a fallback if upstream stalls but we want it in-crate now.

## Alternatives Considered

- **`SyncConnectionWrapper<DualConnection>` as-is** — rejected: blocked (see findings; missing `MoveableBindCollector`/`IntoOwnedRow`, type not nameable).
- **Two separate async connections, user picks** — rejected: abandons write-once, the whole point.
- **`[patch]` a forked diesel for the derive change** — rejected: doesn't reach published consumers (same limitation as ADR DDB-A-0002).

## Implementation Plan

**Pending the strategic choice (A/B/C).** Once chosen, decompose spike-first: (1) spike the riskiest unknown (the missing-traits impl or the async glue) on `Uuid` over both backends; (2) if GO → the connection type + async `RunQueryDsl` integration; (3) async pool; (4) async `#[diesel_dualdb::test]` variant; (5) docs. **Exit criteria:** write-once portable-type queries run async on both backends, pooled, tested.