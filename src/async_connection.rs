//! [`AsyncDualConnection`]: the same write-once queries, async.
//!
//! diesel-async has no async `MultiConnection`, so this module hand-builds
//! one over the **same** `MultiBackend` that `#[derive(MultiConnection)]`
//! generates for [`DualConnection`](crate::DualConnection). Every portable
//! type and bridge impl works unchanged; queries are written against
//! `MultiBackend` and run with `diesel_async::RunQueryDsl`.
//!
//! - **PostgreSQL** runs natively async on diesel-async's
//!   [`AsyncPgConnection`] (tokio-postgres).
//! - **SQLite** runs on diesel-async's [`SyncConnectionWrapper`], which moves
//!   each query onto `spawn_blocking` (SQLite has no async driver).
//!
//! ```no_run
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! use diesel_async::{AsyncConnection, RunQueryDsl};
//! use diesel_dualdb::AsyncDualConnection;
//!
//! let mut conn = AsyncDualConnection::establish("postgres://localhost/app").await?;
//! diesel::sql_query("SELECT 1").execute(&mut conn).await?;
//! # Ok(()) }
//! ```
//!
//! # How a query reaches an arm
//!
//! A query is a `QueryFragment<MultiBackend>`. To run it on one arm, it is
//! wrapped in [`ArmQuery`], a `QueryFragment<Pg>` / `QueryFragment<Sqlite>`
//! that renders the SQL with that arm's `MultiBackend` query builder and
//! collects the binds through that arm's `MultiBackend` bind collector —
//! the same steps the derive's private `SerializedQuery` takes for the sync
//! connection. The Postgres bind pass hands diesel-async's metadata lookup
//! through [`ForeignPgLookup`], which [`DualPgConnection`] accepts.
//!
//! [`DualPgConnection`]: crate::DualPgConnection

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use diesel::backend::Backend;
use diesel::connection::{CacheSize, Instrumentation, TransactionManagerStatus};
use diesel::internal::derives::multiconnection::{AstPassHelper, MultiConnectionHelper};
use diesel::pg::Pg;
use diesel::query_builder::{AsQuery, AstPass, Query, QueryBuilder, QueryFragment, QueryId};
use diesel::row::{Field, PartialRow, Row, RowIndex};
use diesel::sqlite::{Sqlite, SqliteConnection};
use diesel::{ConnectionError, ConnectionResult, QueryResult};
use diesel_async::sync_connection_wrapper::SyncConnectionWrapper;
use diesel_async::{
    AsyncConnection, AsyncConnectionCore, AsyncPgConnection, SimpleAsyncConnection,
    TransactionManager,
};
use futures_util::stream::{BoxStream, StreamExt, TryStreamExt};

use crate::pg::ForeignPgLookup;
use crate::pool::{detect_backend, Backend as UrlBackend};
use crate::{MultiBackend, MultiRawValue};

/// The `MultiBackend` query builder and bind collector. Their enum types live
/// in a private module of the derive's output; these projections are the only
/// way to name them (and their per-arm variants) from here.
type MultiQueryBuilder = <MultiBackend as Backend>::QueryBuilder;
type MultiBindCollector<'a> = <MultiBackend as Backend>::BindCollector<'a>;

/// The SQLite arm's connection type.
pub type AsyncSqliteConnection = SyncConnectionWrapper<SqliteConnection>;

/// An async connection to PostgreSQL or SQLite, with
/// `Backend = MultiBackend`: the async counterpart of
/// [`DualConnection`](crate::DualConnection).
///
/// [`AsyncConnection::establish`] picks the arm from the URL scheme, like
/// [`Pool::connect`](crate::Pool::connect): `postgres://` / `postgresql://` →
/// Postgres; `sqlite://`, `file:`, `:memory:` or a bare path → SQLite.
// The Pg arm is larger, but a connection is long-lived and few: boxing it would
// cost an allocation and change the variant's shape for no real gain.
#[allow(clippy::large_enum_variant)]
pub enum AsyncDualConnection {
    /// PostgreSQL arm: native async.
    Pg(AsyncPgConnection),
    /// SQLite arm: a sync connection run on `spawn_blocking`.
    Sqlite(AsyncSqliteConnection),
}

// ----- query glue -----

/// A `MultiBackend` query, serialized for one arm (the async counterpart of the
/// derive's private `SerializedQuery`).
pub struct ArmQuery<T> {
    inner: T,
    backend: MultiBackend,
}

impl<T> ArmQuery<T> {
    fn pg(inner: T) -> Self {
        ArmQuery {
            inner,
            backend: MultiBackend::Pg(Pg),
        }
    }

    fn sqlite(inner: T) -> Self {
        ArmQuery {
            inner,
            backend: MultiBackend::Sqlite(Sqlite),
        }
    }
}

impl<T: QueryId> QueryId for ArmQuery<T> {
    type QueryId = T::QueryId;
    const HAS_STATIC_QUERY_ID: bool = T::HAS_STATIC_QUERY_ID;
}

impl<T: Query> Query for ArmQuery<T> {
    // As in the derive: the row is decoded by the outer `MultiBackend` query, so
    // the arm query's SQL type does not matter. `Untyped` exists on every backend.
    type SqlType = diesel::sql_types::Untyped;
}

/// Render `inner` with the arm's query builder and mark the pass uncacheable
/// when the inner query is.
fn push_arm_sql<DB: Backend>(
    inner: &impl QueryFragment<MultiBackend>,
    backend: &MultiBackend,
    mut query_builder: MultiQueryBuilder,
    pass: &mut AstPass<'_, '_, DB>,
) -> QueryResult<()> {
    inner.to_sql(&mut query_builder, backend)?;
    pass.push_sql(&query_builder.finish());
    if !inner.is_safe_to_cache_prepared(backend)? {
        pass.unsafe_to_cache_prepared();
    }
    Ok(())
}

impl<T: QueryFragment<MultiBackend>> QueryFragment<Pg> for ArmQuery<T> {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        push_arm_sql(
            &self.inner,
            &self.backend,
            MultiQueryBuilder::Pg(Default::default()),
            &mut pass,
        )?;
        if let Some((outer, lookup)) = pass.bind_collector() {
            // SAFETY: `foreign` is dropped at the end of this block, while
            // `lookup` is still borrowed.
            let mut foreign = unsafe { ForeignPgLookup::new(lookup) };
            let mut collector = MultiBindCollector::Pg(Default::default());
            self.inner
                .collect_binds(&mut collector, &mut foreign, &self.backend)?;
            if let MultiBindCollector::Pg(collector) = collector {
                *outer = collector;
            }
        }
        if let Some((formatter, _)) = pass.debug_binds() {
            let pass = AstPass::<MultiBackend>::collect_debug_binds_pass(formatter, &self.backend);
            self.inner.walk_ast(pass)?;
        }
        Ok(())
    }
}

impl<T: QueryFragment<MultiBackend>> QueryFragment<Sqlite> for ArmQuery<T> {
    fn walk_ast<'b>(&'b self, mut pass: AstPass<'_, 'b, Sqlite>) -> QueryResult<()> {
        push_arm_sql(
            &self.inner,
            &self.backend,
            MultiQueryBuilder::Sqlite(Default::default()),
            &mut pass,
        )?;
        if let Some((outer, lookup)) = pass.bind_collector() {
            let mut collector = MultiBindCollector::Sqlite(Default::default());
            let lookup = <SqliteConnection as MultiConnectionHelper>::to_any(lookup);
            self.inner
                .collect_binds(&mut collector, lookup, &self.backend)?;
            if let MultiBindCollector::Sqlite(collector) = collector {
                *outer = collector;
            }
        }
        if let Some((formatter, _)) = pass.debug_binds() {
            let pass = AstPass::<MultiBackend>::collect_debug_binds_pass(formatter, &self.backend);
            self.inner.walk_ast(pass)?;
        }
        Ok(())
    }
}

// ----- rows -----

type PgRow = <AsyncPgConnection as AsyncConnectionCore>::Row<'static, 'static>;
type SqliteRow = <AsyncSqliteConnection as AsyncConnectionCore>::Row<'static, 'static>;

/// A row loaded through [`AsyncDualConnection`], decoded as `MultiBackend`.
pub enum AsyncDualRow {
    /// A row from the PostgreSQL arm.
    Pg(PgRow),
    /// A row from the SQLite arm.
    Sqlite(SqliteRow),
}

/// A field of an [`AsyncDualRow`].
pub enum AsyncDualField<'f> {
    /// A field from the PostgreSQL arm.
    Pg(<PgRow as Row<'f, Pg>>::Field<'f>),
    /// A field from the SQLite arm.
    Sqlite(<SqliteRow as Row<'f, Sqlite>>::Field<'f>),
}

impl diesel::internal::derives::multiconnection::RowSealed for AsyncDualRow {}

impl<'f> Field<'f, MultiBackend> for AsyncDualField<'f> {
    fn field_name(&self) -> Option<&str> {
        match self {
            AsyncDualField::Pg(f) => f.field_name(),
            AsyncDualField::Sqlite(f) => f.field_name(),
        }
    }

    fn value(&self) -> Option<<MultiBackend as Backend>::RawValue<'_>> {
        match self {
            AsyncDualField::Pg(f) => f.value().map(MultiRawValue::Pg),
            AsyncDualField::Sqlite(f) => f.value().map(MultiRawValue::Sqlite),
        }
    }
}

impl RowIndex<usize> for AsyncDualRow {
    fn idx(&self, idx: usize) -> Option<usize> {
        match self {
            AsyncDualRow::Pg(r) => r.idx(idx),
            AsyncDualRow::Sqlite(r) => r.idx(idx),
        }
    }
}

impl<'c> RowIndex<&'c str> for AsyncDualRow {
    fn idx(&self, idx: &'c str) -> Option<usize> {
        match self {
            AsyncDualRow::Pg(r) => r.idx(idx),
            AsyncDualRow::Sqlite(r) => r.idx(idx),
        }
    }
}

impl<'a> Row<'a, MultiBackend> for AsyncDualRow {
    type Field<'f>
        = AsyncDualField<'f>
    where
        'a: 'f,
        Self: 'f;
    type InnerPartialRow = Self;

    fn field_count(&self) -> usize {
        match self {
            AsyncDualRow::Pg(r) => Row::<Pg>::field_count(r),
            AsyncDualRow::Sqlite(r) => Row::<Sqlite>::field_count(r),
        }
    }

    fn get<'b, I>(&'b self, idx: I) -> Option<Self::Field<'b>>
    where
        'a: 'b,
        Self: RowIndex<I>,
    {
        let idx = self.idx(idx)?;
        match self {
            AsyncDualRow::Pg(r) => Row::<'b, Pg>::get(r, idx).map(AsyncDualField::Pg),
            AsyncDualRow::Sqlite(r) => Row::<'b, Sqlite>::get(r, idx).map(AsyncDualField::Sqlite),
        }
    }

    fn partial_row(&self, range: std::ops::Range<usize>) -> PartialRow<'_, Self> {
        PartialRow::new(self, range)
    }
}

// ----- futures -----

type PgLoad<'c, 'q> = <AsyncPgConnection as AsyncConnectionCore>::LoadFuture<'c, 'q>;
type SqliteLoad<'c, 'q> = <AsyncSqliteConnection as AsyncConnectionCore>::LoadFuture<'c, 'q>;
type PgExecute<'c, 'q> = <AsyncPgConnection as AsyncConnectionCore>::ExecuteFuture<'c, 'q>;
type SqliteExecute<'c, 'q> = <AsyncSqliteConnection as AsyncConnectionCore>::ExecuteFuture<'c, 'q>;

/// The rows of a load, as `MultiBackend` rows.
pub type AsyncDualStream = BoxStream<'static, QueryResult<AsyncDualRow>>;

/// The future of [`AsyncDualConnection`]'s load. The two arms' futures borrow
/// different things (Postgres the connection, SQLite the query), so it holds
/// either rather than boxing both under one lifetime.
pub enum LoadFuture<'conn, 'query> {
    #[doc(hidden)]
    Pg(PgLoad<'conn, 'query>),
    #[doc(hidden)]
    Sqlite(SqliteLoad<'conn, 'query>),
}

impl Future for LoadFuture<'_, '_> {
    type Output = QueryResult<AsyncDualStream>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.get_mut() {
            LoadFuture::Pg(f) => f
                .as_mut()
                .poll(cx)
                .map_ok(|rows| rows.map_ok(AsyncDualRow::Pg).boxed()),
            LoadFuture::Sqlite(f) => f
                .as_mut()
                .poll(cx)
                .map_ok(|rows| rows.map_ok(AsyncDualRow::Sqlite).boxed()),
        }
    }
}

/// The future of [`AsyncDualConnection`]'s execute (see [`LoadFuture`]).
pub enum ExecuteFuture<'conn, 'query> {
    #[doc(hidden)]
    Pg(PgExecute<'conn, 'query>),
    #[doc(hidden)]
    Sqlite(SqliteExecute<'conn, 'query>),
}

impl Future for ExecuteFuture<'_, '_> {
    type Output = QueryResult<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.get_mut() {
            ExecuteFuture::Pg(f) => f.as_mut().poll(cx),
            ExecuteFuture::Sqlite(f) => f.as_mut().poll(cx),
        }
    }
}

// ----- connection -----

impl SimpleAsyncConnection for AsyncDualConnection {
    async fn batch_execute(&mut self, query: &str) -> QueryResult<()> {
        match self {
            AsyncDualConnection::Pg(conn) => conn.batch_execute(query).await,
            AsyncDualConnection::Sqlite(conn) => conn.batch_execute(query).await,
        }
    }
}

impl AsyncConnectionCore for AsyncDualConnection {
    type ExecuteFuture<'conn, 'query> = ExecuteFuture<'conn, 'query>;
    type LoadFuture<'conn, 'query> = LoadFuture<'conn, 'query>;
    type Stream<'conn, 'query> = AsyncDualStream;
    type Row<'conn, 'query> = AsyncDualRow;
    type Backend = MultiBackend;

    fn load<'conn, 'query, T>(&'conn mut self, source: T) -> Self::LoadFuture<'conn, 'query>
    where
        T: AsQuery + 'query,
        T::Query: QueryFragment<MultiBackend> + QueryId + 'query,
    {
        let query = source.as_query();
        match self {
            AsyncDualConnection::Pg(conn) => {
                LoadFuture::Pg(AsyncConnectionCore::load(conn, ArmQuery::pg(query)))
            }
            AsyncDualConnection::Sqlite(conn) => {
                LoadFuture::Sqlite(AsyncConnectionCore::load(conn, ArmQuery::sqlite(query)))
            }
        }
    }

    fn execute_returning_count<'conn, 'query, T>(
        &'conn mut self,
        source: T,
    ) -> Self::ExecuteFuture<'conn, 'query>
    where
        T: QueryFragment<MultiBackend> + QueryId + 'query,
    {
        match self {
            AsyncDualConnection::Pg(conn) => ExecuteFuture::Pg(
                AsyncConnectionCore::execute_returning_count(conn, ArmQuery::pg(source)),
            ),
            AsyncDualConnection::Sqlite(conn) => ExecuteFuture::Sqlite(
                AsyncConnectionCore::execute_returning_count(conn, ArmQuery::sqlite(source)),
            ),
        }
    }
}

impl AsyncConnection for AsyncDualConnection {
    type TransactionManager = Self;

    async fn establish(database_url: &str) -> ConnectionResult<Self> {
        match detect_backend(database_url) {
            Some(UrlBackend::Postgres) => AsyncPgConnection::establish(database_url)
                .await
                .map(AsyncDualConnection::Pg),
            Some(UrlBackend::Sqlite) => {
                // As in `Pool`: diesel's SQLite wants a bare path or a
                // `file:` URI, so strip a `sqlite://` scheme.
                let path = database_url
                    .strip_prefix("sqlite://")
                    .unwrap_or(database_url);
                AsyncSqliteConnection::establish(path)
                    .await
                    .map(AsyncDualConnection::Sqlite)
            }
            None => Err(ConnectionError::InvalidConnectionUrl(format!(
                "unsupported database URL scheme: {database_url}"
            ))),
        }
    }

    fn transaction_state(&mut self) -> &mut Self {
        self
    }

    fn instrumentation(&mut self) -> &mut dyn Instrumentation {
        match self {
            AsyncDualConnection::Pg(conn) => conn.instrumentation(),
            AsyncDualConnection::Sqlite(conn) => conn.instrumentation(),
        }
    }

    fn set_instrumentation(&mut self, instrumentation: impl Instrumentation) {
        match self {
            AsyncDualConnection::Pg(conn) => conn.set_instrumentation(instrumentation),
            AsyncDualConnection::Sqlite(conn) => conn.set_instrumentation(instrumentation),
        }
    }

    fn set_prepared_statement_cache_size(&mut self, size: CacheSize) {
        match self {
            AsyncDualConnection::Pg(conn) => conn.set_prepared_statement_cache_size(size),
            AsyncDualConnection::Sqlite(conn) => conn.set_prepared_statement_cache_size(size),
        }
    }
}

type PgTm = <AsyncPgConnection as AsyncConnection>::TransactionManager;
type SqliteTm = <AsyncSqliteConnection as AsyncConnection>::TransactionManager;

/// Transactions dispatch to the active arm's own transaction manager (the
/// same shape as the derive's sync `MultiConnection`).
impl TransactionManager<AsyncDualConnection> for AsyncDualConnection {
    type TransactionStateData = Self;

    async fn begin_transaction(conn: &mut Self) -> QueryResult<()> {
        match conn {
            AsyncDualConnection::Pg(c) => PgTm::begin_transaction(c).await,
            AsyncDualConnection::Sqlite(c) => SqliteTm::begin_transaction(c).await,
        }
    }

    async fn rollback_transaction(conn: &mut Self) -> QueryResult<()> {
        match conn {
            AsyncDualConnection::Pg(c) => PgTm::rollback_transaction(c).await,
            AsyncDualConnection::Sqlite(c) => SqliteTm::rollback_transaction(c).await,
        }
    }

    async fn commit_transaction(conn: &mut Self) -> QueryResult<()> {
        match conn {
            AsyncDualConnection::Pg(c) => PgTm::commit_transaction(c).await,
            AsyncDualConnection::Sqlite(c) => SqliteTm::commit_transaction(c).await,
        }
    }

    fn transaction_manager_status_mut(conn: &mut Self) -> &mut TransactionManagerStatus {
        match conn {
            AsyncDualConnection::Pg(c) => PgTm::transaction_manager_status_mut(c),
            AsyncDualConnection::Sqlite(c) => SqliteTm::transaction_manager_status_mut(c),
        }
    }
}
