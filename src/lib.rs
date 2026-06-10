//! diesel-dualdb: write Diesel query code once, run it on both PostgreSQL and
//! SQLite.
//!
//! Built on diesel's `#[derive(MultiConnection)]`: [`DualConnection`] is the
//! unified connection, and this crate bridges portable types
//! ([`types`]) onto the generated `MultiBackend` so `get_result`/`RETURNING`
//! work on one arm against either backend.
//!
//! Both-backend tests are written with the [`test`] attribute:
//!
//! ```ignore
//! #[diesel_dualdb::test(pg, sqlite)]
//! fn it_round_trips(conn: &mut diesel_dualdb::DualConnection) { /* … */ }
//! ```

// Lets macro-generated code (and downstream users) refer to this crate by its
// canonical name `::diesel_dualdb` even from within the crate itself.
extern crate self as diesel_dualdb;

pub mod backend;
pub mod escape;
pub mod pool;
pub mod sql_types;
pub mod types;

/// A connection pool with backend detection. See [`pool`].
pub use pool::Pool;

/// `#[diesel_dualdb::test(pg, sqlite)]` — run one test body against each
/// backend. See [`diesel_dualdb_macros::test`].
pub use diesel_dualdb_macros::test;

/// `diesel_dualdb::bridge!(Marker, Newtype)` — generate the `MultiBackend`
/// bridge for a non-generic portable type. See [`diesel_dualdb_macros::bridge`].
pub use diesel_dualdb_macros::bridge;

/// `#[derive(diesel_dualdb::DualEnum)]` — a portable enum (PostgreSQL native
/// `enum` / SQLite `TEXT`). See [`diesel_dualdb_macros::DualEnum`].
pub use diesel_dualdb_macros::DualEnum;

/// The canonical dual-backend connection.
///
/// `#[derive(MultiConnection)]` generates an enum `Connection` impl plus the
/// associated `MultiBackend`. The crate owns this type, which is what lets us
/// implement the bridge traits (`HasSqlType`/`ToSql`/`FromSql`) on
/// `MultiBackend` locally, with no orphan-rule problem.
#[derive(diesel::MultiConnection)]
pub enum DualConnection {
    /// PostgreSQL arm.
    Pg(diesel::PgConnection),
    /// SQLite arm.
    Sqlite(diesel::SqliteConnection),
}
