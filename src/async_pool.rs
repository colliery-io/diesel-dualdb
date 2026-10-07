//! Async connection pooling with backend detection: the async counterpart of
//! [`pool`](crate::pool).
//!
//! [`AsyncPool::connect`] inspects the database URL, picks the backend, and
//! returns a [deadpool](https://docs.rs/deadpool) pool (via diesel-async's
//! integration) yielding [`AsyncDualConnection`]:
//!
//! ```no_run
//! # async fn demo() -> Result<(), diesel_dualdb::async_pool::Error> {
//! let pool = diesel_dualdb::AsyncPool::connect("postgres://localhost/app")?;
//! let mut conn = pool.get().await?;   // a pooled AsyncDualConnection
//! // use `&mut *conn` anywhere a `&mut AsyncDualConnection` is wanted
//! # Ok(()) }
//! ```
//!
//! deadpool opens connections on demand, so `connect` validates only the URL
//! scheme: an unreachable database surfaces on the first [`AsyncPool::get`]
//! (like the sync [`Builder::connect_lazy`](crate::pool::Builder::connect_lazy)).

use std::fmt;
use std::time::Duration;

use diesel_async::pooled_connection::deadpool::{BuildError, Object, Pool, PoolBuilder, PoolError};
use diesel_async::pooled_connection::{AsyncDieselConnectionManager, PoolableConnection};

use crate::pool::detect_backend;
use crate::AsyncDualConnection;

/// Pooled connections are checked with `SELECT 1` on recycle, and dropped when
/// their transaction manager is broken (diesel-async's defaults).
impl PoolableConnection for AsyncDualConnection {}

/// The deadpool manager for [`AsyncDualConnection`].
pub type AsyncDualConnectionManager = AsyncDieselConnectionManager<AsyncDualConnection>;

/// A pooled connection — derefs to [`AsyncDualConnection`].
pub type AsyncPooledConnection = Object<AsyncDualConnection>;

/// A pool of [`AsyncDualConnection`]s.
#[derive(Clone)]
pub struct AsyncPool(Pool<AsyncDualConnection>);

impl AsyncPool {
    /// Detect the backend from `url` and build a pool with default settings.
    pub fn connect(url: &str) -> Result<Self, Error> {
        Self::builder().connect(url)
    }

    /// Start configuring a pool (`max_size`, timeouts) before connecting.
    pub fn builder() -> AsyncBuilder {
        AsyncBuilder {
            max_size: None,
            timeout: None,
        }
    }

    /// Check out a pooled connection, opening one if none is idle.
    pub async fn get(&self) -> Result<AsyncPooledConnection, Error> {
        self.0.get().await.map_err(Error::Pool)
    }

    /// The underlying deadpool pool, for knobs not surfaced here.
    pub fn inner(&self) -> &Pool<AsyncDualConnection> {
        &self.0
    }
}

/// Builder for [`AsyncPool`]. Proxies the common deadpool settings; for
/// anything else, build a deadpool `Pool` directly over
/// [`AsyncDualConnectionManager`].
pub struct AsyncBuilder {
    max_size: Option<usize>,
    timeout: Option<Duration>,
}

impl AsyncBuilder {
    /// Maximum number of pooled connections.
    pub fn max_size(mut self, n: usize) -> Self {
        self.max_size = Some(n);
        self
    }

    /// Maximum time a checkout waits for a free slot, and for a new connection
    /// to be established, before erroring.
    pub fn connection_timeout(mut self, t: Duration) -> Self {
        self.timeout = Some(t);
        self
    }

    /// Detect the backend from `url` and build the pool. No connection is
    /// opened until the first checkout.
    pub fn connect(self, url: &str) -> Result<AsyncPool, Error> {
        detect_backend(url).ok_or_else(|| Error::UnknownUrl(url.to_owned()))?;
        let mut builder: PoolBuilder<AsyncDualConnection> =
            Pool::builder(AsyncDualConnectionManager::new(url));
        if let Some(n) = self.max_size {
            builder = builder.max_size(n);
        }
        if let Some(t) = self.timeout {
            builder = builder
                .wait_timeout(Some(t))
                .create_timeout(Some(t))
                .runtime(deadpool::Runtime::Tokio1);
        }
        builder.build().map(AsyncPool).map_err(Error::Build)
    }
}

/// Errors from [`AsyncPool`] construction and checkout.
#[derive(Debug)]
pub enum Error {
    /// The URL's scheme didn't match a known backend.
    UnknownUrl(String),
    /// A deadpool build error.
    Build(BuildError),
    /// A checkout error (includes the underlying connection error).
    Pool(PoolError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnknownUrl(url) => {
                write!(f, "unrecognized database URL (no known backend): {url}")
            }
            Error::Build(e) => write!(f, "{e}"),
            Error::Pool(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::UnknownUrl(_) => None,
            Error::Build(e) => Some(e),
            Error::Pool(e) => Some(e),
        }
    }
}
