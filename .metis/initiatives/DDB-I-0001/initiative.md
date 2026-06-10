---
id: spike-uuid-through-the
level: initiative
title: "Spike: Uuid through the MultiBackend bridge"
short_code: "DDB-I-0001"
created_at: 2026-06-07T14:57:22.039654+00:00
updated_at: 2026-06-08T15:21:52.250970+00:00
parent: DDB-V-0001
blocked_by: []
archived: false

tags:
  - "#initiative"
  - "#phase/completed"


exit_criteria_met: false
estimated_complexity: S
initiative_id: spike-uuid-through-the
---

# Spike: Uuid through the MultiBackend bridge Initiative

## Context

This spike gates the entire crate. The bet (DESIGN.md §4, vision DDB-V-0001) is that Diesel's documented `MultiConnection` extension point — implementing `HasSqlType`/`ToSql`/`FromSql` for a custom type against the generated `MultiBackend` — actually works end-to-end for `get_result`/`RETURNING` and pooling. If it does, write-once queries are real and M0–M2 proceed as designed. If it doesn't, the architecture changes (and M0's type layer still has standalone value).

The riskiest unknown is the `FromSql<_, MultiBackend>` plumbing: matching the generated `MultiBackend::RawValue` and dispatching to the inner backend's deserialization. Secondary unknown: whether `DualConnection` gets a usable `R2D2Connection` impl from the derive.

**Reference material:** `../cloacina` has hand-rolled per-backend type impls to crib from (`src/database/universal_types.rs`) and shows the duplication we're eliminating (`src/database/connection/backend.rs`, `src/dal/unified/`). Diesel's `MultiConnection` derive docs contain the `MyEnum`/`MyInteger` extension example — that's the pattern to follow.

## Goals & Non-Goals

**Goals:**
- Prove `sql_types::Uuid` + `types::Uuid` work through `DualConnection` on **one arm**: `get_result`, insert-with-`RETURNING`, `.filter(...eq(uuid))` — on both Pg and SQLite.
- The RETURNING test must use a **mixed row**: a portable Uuid column alongside native Text/Integer columns (the realistic shape; metadata-lookup interactions hide there).
- Confirm `DualConnection` works with r2d2 pooling (`R2D2Connection` impl from the derive or hand-written).
- Answer: **is `Bool` already in `MultiBackend`'s required core set?** (If yes, drop `types::Bool` from M0 scope.)
- Form a first opinion on the native-type compatibility shim (vision constraint 4): does bridging `diesel::sql_types::Uuid` itself onto `MultiBackend` look feasible, or does it collide with the derive's internals?

**Non-Goals:**
- Other portable types (Timestamp/Bytes/Json) — M0.
- API polish, feature flags, docs — throwaway-quality code is acceptable; findings are the deliverable.
- Async, migrations, upsert helpers.

## Detailed Design

1. Scaffold the crate: `cargo init --lib`, diesel 2.2.x with `postgres`, `sqlite`, `r2d2`, `uuid` features; pin versions.
2. `#[derive(diesel::MultiConnection)] pub enum DualConnection { Pg(PgConnection), Sqlite(SqliteConnection) }`.
3. Marker type `sql_types::Uuid` (`#[diesel(postgres_type(name = "uuid"))]`, `#[diesel(sqlite_type(name = "Binary"))]`) + newtype `types::Uuid(uuid::Uuid)`.
4. Impls: `ToSql`/`FromSql` for `Pg` (delegate to diesel's native uuid support), for `Sqlite` (16-byte BLOB), then the bridge: `HasSqlType<sql_types::Uuid> for MultiBackend`, `ToSql<_, MultiBackend>`, `FromSql<_, MultiBackend>` following the derive docs' extension example.
5. Spike test: small table (`id` uuid PK, `name` text, `count` integer) created via raw SQL on each backend; insert with RETURNING, select with filter-by-uuid, round-trip assert. SQLite in-memory; Postgres env-gated via `DUALDB_PG_URL` (skip if unset).
6. Pooling test: `r2d2::Pool` over a `ConnectionManager<DualConnection>`-equivalent; acquire and run a query.

## Alternatives Considered

- **Build M0 first, bridge later:** rejected — the bridge is the bet; building five types before knowing the bridge works risks rework of the type-layer conventions (how markers/newtypes are shaped is informed by what the bridge needs).
- **Fork/patch Diesel to fix #4079 upstream:** rejected for v1 — upstream timeline is unowned; the extension point is documented and local. Upstreaming learnings later is still open.
- **Concrete-connection dispatch (cloacina status quo):** the explicitly rejected failure mode; see vision principles.

## Implementation Plan

Single-task spike, expected a few focused sessions:

1. Scaffold + DualConnection derive compiles.
2. Uuid impls for Pg/Sqlite + round-trip on concrete connections.
3. Bridge impls on MultiBackend + the one-arm RETURNING test (the make-or-break step).
4. Pooling check.
5. Bool/native-shim questions answered.
6. **Spike report written back into this initiative** (findings, exact trait bounds used, gotchas) — this is the primary deliverable and feeds M0/M1 design.

**Exit criteria:** one-arm `get_result`/RETURNING with mixed row passes on both backends; pooling works; Bool question answered; spike report recorded here. If blocked: documented failure analysis + recommended architecture change.

---

## SPIKE REPORT — 2026-06-08 — ✅ GO

**Verdict: the bet paid off. Build the crate as designed.** Write-once `get_result`/`RETURNING` runs through `DualConnection` on one arm, both backends, with no per-backend `match`. All exit criteria met; 8 tests green via `angreal test all` (SQLite in-memory + real Postgres container).

Tasks: [[DDB-T-0001]] scaffold · [[DDB-T-0005]] test orchestration · [[DDB-T-0002]] concrete Uuid type · [[DDB-T-0003]] the bridge · [[DDB-T-0004]] pooling + this report.

### 1. The bridge is mechanical, not high-risk

The feared part — `FromSql<_, MultiBackend>` matching the generated `MultiRawValue` — is **pre-solved by the derive**. `cargo expand` shows `#[derive(MultiConnection)]` emits two *public* helpers that do the per-arm dispatch:

- `MultiBackend::lookup_sql_type::<ST>(lookup)` — backs `HasSqlType`.
- `MultiRawValue::from_sql::<T, ST>()` — backs `FromSql` (we never match `MultiRawValue` variants ourselves).

So the three bridge impls per type are one-liners, *identical in shape* to what the derive generates for the core types. Exact working forms (`src/backend.rs`):

```rust
impl HasSqlType<sql_types::Uuid> for MultiBackend {
    fn metadata(lookup: &mut Self::MetadataLookup) -> Self::TypeMetadata {
        Self::lookup_sql_type::<sql_types::Uuid>(lookup)
    }
}
impl ToSql<sql_types::Uuid, MultiBackend> for types::Uuid {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, MultiBackend>) -> serialize::Result {
        out.set_value((sql_types::Uuid, self));
        Ok(IsNull::No)
    }
}
impl FromSql<sql_types::Uuid, MultiBackend> for types::Uuid {
    fn from_sql(bytes: <MultiBackend as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        bytes.from_sql::<Self, sql_types::Uuid>()
    }
}
```

`crate::MultiBackend` and `crate::MultiRawValue` are re-exported (`pub use`); `MultiBackend` is defined in-crate, so **no orphan-rule problem** — confirmed. The concrete per-arm `To/FromSql` for `Pg`/`Sqlite` they delegate to are in `src/types/uuid.rs` (signatures cribbed from cloacina; compiled first try).

### 2. ⚠️ `returning_clauses_for_sqlite_3_35` is a REQUIRED diesel feature

The single non-obvious gotcha. `QueryFragment<MultiBackend>` for a RETURNING clause requires **both** arms to support RETURNING; diesel gates SQLite's behind this feature (needs SQLite ≥ 3.35). Without it, the first one-arm `get_result` fails to compile (`QueryFragment<Sqlite, DoesNotSupportReturningClause>` unsatisfied). **It must be a hard, non-optional feature of the published crate** — the whole one-arm-RETURNING premise depends on it. → M0 feature-flag task + crate's required features.

### 3. Bool already supported ⇒ drop `types::Bool`

The derive's required core set on `MultiBackend` already includes `HasSqlType<Bool>` (plus SmallInt/Integer/BigInt/Float/Double/Text/Binary/Date/Time/Timestamp). **`Uuid` is absent — it is exactly the #4079 gap.** ⇒ **`types::Bool` is dropped from M0 scope.**

### 4. Pooling works out of the box

The derive provides `R2D2Connection` for `DualConnection`, so `r2d2::Pool<ConnectionManager<DualConnection>>` works directly and the bridge runs through pooled connections ([[DDB-T-0004]], `tests/pool.rs`, both backends).

### 5. Native-shim: feasible, deferred

Bridging diesel's *native* `diesel::sql_types::Uuid` onto `MultiBackend` is mechanically identical (helpers are generic over `ST`). Risk is import shadowing + picking SQLite's representation for the native type — not trait expressibility. Feasible; defer per vision constraint 4.

### Findings that adjust downstream plans

- **Naive `establish` routing:** the derived `Connection::establish` tries **Pg first, then Sqlite** (no scheme inspection). Reinforces M2's explicit `Pool::connect` URL/scheme detection ([[DDB-I-0004]]) — it's a real need, not polish.
- **Every later type is the same 3-line bridge** → a `dualdb-macros` codegen pass is attractive but optional (M1, [[DDB-I-0003]]).
- **Test patterns to reuse:** `tests/bridge.rs` (ORM-level, backend-agnostic `&mut DualConnection` fn) and the `angreal test all` orchestration are the templates for M0/M1.
- **diesel pinned at 2.3.10** (design said 2.2.x; 2.3 is current and works); **MSRV 1.84 provisional** — verify in M0.

**Next:** human checkpoint to close this spike and start M0 ([[DDB-I-0002]]) — which now drops Bool and carries the required-feature + MSRV items forward.