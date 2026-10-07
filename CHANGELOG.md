# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `async` feature: `AsyncDualConnection`, an async connection with
  `Backend = MultiBackend` over diesel-async. Postgres is natively async and
  SQLite runs on `spawn_blocking`. Every portable type and the bridge work
  unchanged, including RETURNING and transactions (with savepoints).
- `AsyncPool`: a deadpool pool of `AsyncDualConnection` with URL/scheme
  detection, the async counterpart of `Pool`.
- `#[diesel_dualdb::test]` on an `async fn(conn: &mut AsyncDualConnection)`.
- Docs: how-to "Use async", explanation "Async".

### Changed

- **Breaking:** the Postgres arm of `DualConnection` is now
  `DualConnection::Pg(DualPgConnection)`, a newtype over `diesel::PgConnection`.
  It implements `Connection` and derefs to `PgConnection`, so most code is
  unchanged. Code that builds the arm directly must wrap the connection:
  `DualConnection::Pg(DualPgConnection(pg))` or `DualConnection::Pg(pg.into())`.
  `dispatch` still passes `&mut PgConnection`.

## [0.1.0]

First release: the portable type layer (Uuid, Bytes, Timestamp, Json, Decimal,
Array, DualEnum), the `MultiBackend` bridge, `#[diesel_dualdb::test]`,
`bridge!`, the schema generator, `Pool::connect`, and the `dispatch` escape
hatch.
