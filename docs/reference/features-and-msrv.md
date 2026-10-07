# Reference: Cargo features & MSRV

## Crate features

| Feature | Enables | Default |
|---|---|---|
| `uuid` | the `Uuid` portable type + `diesel/uuid` | yes |
| `chrono` | the `Timestamp` portable type + `diesel/chrono` | yes |
| `serde_json` | the `Json<T>` portable type + `diesel/serde_json` (+ `serde`) | yes |
| `decimal` | the `Decimal` portable type + `diesel/numeric` (+ `bigdecimal`) | yes |
| `array` | the `Array<T>` portable type (uses `serde_json`) | yes |
| `async` | `AsyncDualConnection`, `AsyncPool`, and `#[diesel_dualdb::test]` on `async fn` (+ `diesel-async`, `deadpool`, `tokio`) | no |

Each type feature also turns on diesel's matching integration. The default set
is every portable type. `Bytes` has no feature — it's always available. `async`
is opt-in so sync users don't pull in tokio.

`--no-default-features` gives the bare crate (`DualConnection` + the bridge
machinery) with no portable types.

## Required diesel features

The crate depends on `diesel` with `postgres`, `sqlite`, `r2d2`, and
**`returning_clauses_for_sqlite_3_35`** — the last is **not optional**:
`get_result`/`RETURNING` through `MultiBackend` requires *both* arms to support a
`RETURNING` clause, and diesel gates SQLite's behind that feature (needs SQLite
≥ 3.35).

## MSRV

**Rust 1.86.** This is diesel 2.3's declared minimum, which binds the crate.

## diesel version

`diesel` 2.3.x. `MultiConnection` (the foundation) has been available since 2.1.

With `async`: `diesel-async` 0.9.x (features `postgres`, `sqlite`, `deadpool`).
