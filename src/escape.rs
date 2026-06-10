//! The escape hatch: a tidy way to run per-backend code on the rare query that
//! genuinely cannot be shared.
//!
//! [`DualConnection::dispatch`](crate::DualConnection::dispatch) takes one
//! closure per backend and runs the active arm. Both arms are required, so
//! divergence stays **explicit and exhaustive** — the helper makes the split
//! readable, it does not hide it.
//!
//! ```no_run
//! use diesel::prelude::*;
//! # fn demo(conn: &mut diesel_dualdb::DualConnection) -> diesel::QueryResult<usize> {
//! # diesel::table! { kv (k) { k -> diesel::sql_types::Text, v -> diesel::sql_types::Integer } }
//! // `on_conflict` isn't supported through MultiBackend, so upsert per backend:
//! conn.dispatch(
//!     |pg| diesel::insert_into(kv::table)
//!         .values((kv::k.eq("a"), kv::v.eq(1)))
//!         .on_conflict(kv::k).do_update().set(kv::v.eq(1))
//!         .execute(pg),
//!     |sqlite| diesel::insert_into(kv::table)
//!         .values((kv::k.eq("a"), kv::v.eq(1)))
//!         .on_conflict(kv::k).do_update().set(kv::v.eq(1))
//!         .execute(sqlite),
//! )
//! # }
//! ```

use diesel::{PgConnection, SqliteConnection};

use crate::DualConnection;

impl DualConnection {
    /// Run a per-backend closure on the active connection arm.
    ///
    /// Both closures are required, so the compiler enforces that divergence is
    /// handled for both backends. Use this only when the shared
    /// `&mut DualConnection` path genuinely can't express the operation
    /// (locking hints, `ON CONFLICT`, recursive CTEs, FTS, …) — most code
    /// should stay on one arm.
    pub fn dispatch<R>(
        &mut self,
        pg: impl FnOnce(&mut PgConnection) -> R,
        sqlite: impl FnOnce(&mut SqliteConnection) -> R,
    ) -> R {
        match self {
            DualConnection::Pg(conn) => pg(conn),
            DualConnection::Sqlite(conn) => sqlite(conn),
        }
    }
}
