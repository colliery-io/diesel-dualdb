//! The PostgreSQL arm of [`DualConnection`](crate::DualConnection).
//!
//! [`DualPgConnection`] is a thin newtype over [`diesel::PgConnection`]. It
//! exists for one reason: the `MultiBackend` that `#[derive(MultiConnection)]`
//! generates finds the Postgres type-metadata lookup by downcasting it to the
//! arm's connection type (`MultiConnectionHelper::from_any`). For a plain
//! `PgConnection` arm that downcast accepts only a sync `PgConnection`, so the
//! bridge could never collect Postgres binds for diesel-async's
//! `AsyncPgConnection`, whose lookup is a different type. Owning the arm type
//! lets [`from_any`](MultiConnectionHelper::from_any) also accept that lookup
//! (wrapped for the length of one bind pass), which is what makes
//! `AsyncDualConnection` (feature `async`) possible.
//!
//! Everything else delegates to the inner `PgConnection`, and the newtype
//! derefs to it, so Postgres-only APIs stay one `&mut *conn` away.

use std::any::Any;
use std::ops::{Deref, DerefMut};

use diesel::connection::{
    AnsiTransactionManager, CacheSize, Connection, DefaultLoadingMode, Instrumentation,
    LoadConnection, SimpleConnection,
};
use diesel::expression::QueryMetadata;
use diesel::internal::derives::multiconnection::{ConnectionSealed, MultiConnectionHelper};
use diesel::pg::{Pg, PgMetadataLookup};
use diesel::query_builder::{Query, QueryFragment, QueryId};
use diesel::sql_types::TypeMetadata;
use diesel::{ConnectionResult, PgConnection, QueryResult};

/// The PostgreSQL arm of [`DualConnection`](crate::DualConnection): a
/// [`diesel::PgConnection`] that the `MultiBackend` bridge can also drive
/// asynchronously.
///
/// It implements [`Connection`] by delegating to the inner connection, and
/// derefs to [`PgConnection`] for Postgres-only APIs.
pub struct DualPgConnection(pub PgConnection);

impl From<PgConnection> for DualPgConnection {
    fn from(conn: PgConnection) -> Self {
        DualPgConnection(conn)
    }
}

impl Deref for DualPgConnection {
    type Target = PgConnection;

    fn deref(&self) -> &PgConnection {
        &self.0
    }
}

impl DerefMut for DualPgConnection {
    fn deref_mut(&mut self) -> &mut PgConnection {
        &mut self.0
    }
}

impl SimpleConnection for DualPgConnection {
    fn batch_execute(&mut self, query: &str) -> QueryResult<()> {
        self.0.batch_execute(query)
    }
}

impl ConnectionSealed for DualPgConnection {}

impl Connection for DualPgConnection {
    type Backend = Pg;
    type TransactionManager = AnsiTransactionManager;

    fn establish(database_url: &str) -> ConnectionResult<Self> {
        PgConnection::establish(database_url).map(DualPgConnection)
    }

    fn execute_returning_count<T>(&mut self, source: &T) -> QueryResult<usize>
    where
        T: QueryFragment<Pg> + QueryId,
    {
        self.0.execute_returning_count(source)
    }

    fn transaction_state(&mut self) -> &mut AnsiTransactionManager {
        self.0.transaction_state()
    }

    fn instrumentation(&mut self) -> &mut dyn Instrumentation {
        self.0.instrumentation()
    }

    fn set_instrumentation(&mut self, instrumentation: impl Instrumentation) {
        self.0.set_instrumentation(instrumentation)
    }

    fn set_prepared_statement_cache_size(&mut self, size: CacheSize) {
        self.0.set_prepared_statement_cache_size(size)
    }
}

impl LoadConnection<DefaultLoadingMode> for DualPgConnection {
    type Cursor<'conn, 'query> = <PgConnection as LoadConnection>::Cursor<'conn, 'query>;
    type Row<'conn, 'query> = <PgConnection as LoadConnection>::Row<'conn, 'query>;

    fn load<'conn, 'query, T>(
        &'conn mut self,
        source: T,
    ) -> QueryResult<Self::Cursor<'conn, 'query>>
    where
        T: Query + QueryFragment<Pg> + QueryId + 'query,
        Pg: QueryMetadata<T::SqlType>,
    {
        <PgConnection as LoadConnection>::load(&mut self.0, source)
    }
}

impl MultiConnectionHelper for DualPgConnection {
    fn to_any<'a>(lookup: &mut <Pg as TypeMetadata>::MetadataLookup) -> &mut (dyn Any + 'a) {
        // The sync path: diesel hands the `PgConnection` itself in as the lookup.
        lookup.as_any()
    }

    fn from_any(lookup: &mut dyn Any) -> Option<&mut (dyn PgMetadataLookup + 'static)> {
        if lookup.is::<PgConnection>() {
            return lookup
                .downcast_mut::<PgConnection>()
                .map(|conn| conn as &mut (dyn PgMetadataLookup + 'static));
        }
        lookup.downcast_mut::<ForeignPgLookup>().map(|foreign| {
            // SAFETY: `ForeignPgLookup::new`'s contract keeps the pointee alive
            // for as long as the wrapper, and the returned borrow is tied to
            // the borrow of the wrapper.
            unsafe { &mut *foreign.ptr }
        })
    }
}

impl diesel::r2d2::R2D2Connection for DualPgConnection {
    fn ping(&mut self) -> QueryResult<()> {
        self.0.ping()
    }

    fn is_broken(&mut self) -> bool {
        self.0.is_broken()
    }
}

impl diesel::migration::MigrationConnection for DualPgConnection {
    fn setup(&mut self) -> QueryResult<usize> {
        self.0.setup()
    }
}

/// A Postgres metadata lookup that is not a sync `PgConnection` (in practice,
/// diesel-async's), held by raw pointer so the wrapper is `'static` and can
/// travel through the `&mut dyn Any` that `MultiBackend` bind collection
/// passes around.
#[cfg_attr(not(feature = "async"), allow(dead_code))]
pub(crate) struct ForeignPgLookup {
    ptr: *mut (dyn PgMetadataLookup + 'static),
}

#[cfg_attr(not(feature = "async"), allow(dead_code))]
impl ForeignPgLookup {
    /// Wrap `lookup` for one bind-collection pass.
    ///
    /// # Safety
    ///
    /// The wrapper must not outlive `lookup`: build it, pass it to
    /// `collect_binds`, and drop it before `lookup`'s borrow ends.
    pub(crate) unsafe fn new(lookup: &mut (dyn PgMetadataLookup + 'static)) -> Self {
        ForeignPgLookup { ptr: lookup }
    }
}
