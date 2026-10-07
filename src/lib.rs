//! diesel-dualdb: write Diesel query code once, run it on both PostgreSQL and
//! SQLite.
//!
//! Built on diesel's `#[derive(MultiConnection)]`: [`DualConnection`] is the
//! unified connection, and this crate bridges portable types
//! ([`types`]) onto the generated `MultiBackend` so `get_result`/`RETURNING`
//! work on one arm against either backend.
//!
//! With the `async` feature, `AsyncDualConnection` (and `AsyncPool`) run the
//! same queries through diesel-async: see the `async_connection` module.
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

#[cfg(feature = "async")]
pub mod async_connection;
#[cfg(feature = "async")]
pub mod async_pool;
pub mod backend;
pub mod escape;
pub mod pg;
pub mod pool;
pub mod sql_types;
pub mod types;

/// A connection pool with backend detection. See [`pool`].
pub use pool::Pool;

/// The PostgreSQL arm of [`DualConnection`]. See [`pg`].
pub use pg::DualPgConnection;

/// The async dual-backend connection. See [`async_connection`].
#[cfg(feature = "async")]
pub use async_connection::AsyncDualConnection;

/// An async connection pool with backend detection. See [`async_pool`].
#[cfg(feature = "async")]
pub use async_pool::AsyncPool;

/// `#[diesel_dualdb::test(pg, sqlite)]` — run one test body against each
/// backend. See [`diesel_dualdb_macros::test`].
pub use diesel_dualdb_macros::test;

/// `diesel_dualdb::bridge!(Marker, Newtype)` — generate the `MultiBackend`
/// bridge for a non-generic portable type. See [`diesel_dualdb_macros::bridge`].
pub use diesel_dualdb_macros::bridge;

/// `#[derive(diesel_dualdb::DualEnum)]` — a portable enum (PostgreSQL native
/// `enum` / SQLite `TEXT`). See [`diesel_dualdb_macros::DualEnum`].
pub use diesel_dualdb_macros::DualEnum;

/// Support code for `#[diesel_dualdb::test]` on an `async fn`. Not public API.
#[cfg(feature = "async")]
#[doc(hidden)]
pub mod __private {
    pub use diesel_async::AsyncConnection;

    /// Drive a test body to completion on a fresh tokio runtime.
    pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("dualdb::test: build tokio runtime")
            .block_on(future)
    }
}

/// The canonical dual-backend connection.
///
/// `#[derive(MultiConnection)]` generates an enum `Connection` impl plus the
/// associated `MultiBackend`. The crate owns this type, which is what lets us
/// implement the bridge traits (`HasSqlType`/`ToSql`/`FromSql`) on
/// `MultiBackend` locally, with no orphan-rule problem.
#[derive(diesel::MultiConnection)]
pub enum DualConnection {
    /// PostgreSQL arm: a [`diesel::PgConnection`], wrapped in
    /// [`DualPgConnection`] (which derefs to it).
    Pg(DualPgConnection),
    /// SQLite arm.
    Sqlite(diesel::SqliteConnection),
}
