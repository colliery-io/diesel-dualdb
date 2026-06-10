//! Connection pooling with backend detection.
//!
//! [`Pool::connect`] inspects the database URL, picks the backend, and returns
//! an r2d2 pool yielding [`DualConnection`]:
//!
//! ```no_run
//! let pool = diesel_dualdb::Pool::connect("postgres://localhost/app")?;
//! let mut conn = pool.get()?;            // a pooled DualConnection
//! // use `&mut *conn` anywhere a `&mut DualConnection` is wanted
//! # Ok::<(), diesel_dualdb::pool::Error>(())
//! ```
//!
//! Unlike the connection's derived `establish` (which tries Postgres, then
//! SQLite), the pool establishes exactly the detected backend.

use std::fmt;

use diesel::prelude::*;
use diesel::r2d2::{ManageConnection, PoolError, R2D2Connection};

use crate::DualConnection;

/// Which backend a URL refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Postgres,
    Sqlite,
}

/// Detect the backend from a database URL.
///
/// - `postgres://` / `postgresql://` → [`Backend::Postgres`]
/// - `sqlite://`, `file:`, `:memory:`, or a bare path → [`Backend::Sqlite`]
/// - any other scheme (`mysql://`, …) → `None`
pub fn detect_backend(url: &str) -> Option<Backend> {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
        Some(Backend::Postgres)
    } else if lower.starts_with("sqlite://") || lower.starts_with("file:") || lower == ":memory:" {
        Some(Backend::Sqlite)
    } else if lower.contains("://") {
        None // an explicit, unrecognized scheme
    } else {
        Some(Backend::Sqlite) // no scheme → a filesystem path
    }
}

/// An r2d2 connection manager that establishes a fixed backend.
#[derive(Debug, Clone)]
pub struct DualConnectionManager {
    url: String,
    backend: Backend,
}

impl DualConnectionManager {
    /// Build a manager, detecting the backend from `url`.
    pub fn new(url: &str) -> Result<Self, Error> {
        let backend = detect_backend(url).ok_or_else(|| Error::UnknownUrl(url.to_owned()))?;
        Ok(Self {
            url: url.to_owned(),
            backend,
        })
    }
}

impl ManageConnection for DualConnectionManager {
    type Connection = DualConnection;
    type Error = diesel::r2d2::Error;

    fn connect(&self) -> Result<DualConnection, Self::Error> {
        let conn = match self.backend {
            Backend::Postgres => PgConnection::establish(&self.url).map(DualConnection::Pg),
            Backend::Sqlite => {
                // `detect_backend` accepts a `sqlite://` URL, but diesel's
                // `SqliteConnection::establish` wants a bare path or a `file:`
                // URI — `sqlite://./app.db` would open a file literally named
                // that. Strip the `sqlite://` scheme so both forms work.
                let path = self.url.strip_prefix("sqlite://").unwrap_or(&self.url);
                SqliteConnection::establish(path).map(DualConnection::Sqlite)
            }
        };
        conn.map_err(diesel::r2d2::Error::ConnectionError)
    }

    fn is_valid(&self, conn: &mut DualConnection) -> Result<(), Self::Error> {
        conn.ping().map_err(diesel::r2d2::Error::QueryError)
    }

    fn has_broken(&self, conn: &mut DualConnection) -> bool {
        conn.is_broken()
    }
}

/// A pool of [`DualConnection`]s.
#[derive(Clone)]
pub struct Pool(diesel::r2d2::Pool<DualConnectionManager>);

/// A pooled connection — derefs to [`DualConnection`].
pub type PooledConnection = diesel::r2d2::PooledConnection<DualConnectionManager>;

impl Pool {
    /// Detect the backend from `url` and build a pool with default settings.
    pub fn connect(url: &str) -> Result<Self, Error> {
        Self::builder().connect(url)
    }

    /// Start configuring a pool (`max_size`, timeouts, …) before connecting.
    pub fn builder() -> Builder {
        Builder {
            inner: diesel::r2d2::Pool::builder(),
        }
    }

    /// Check out a pooled connection.
    pub fn get(&self) -> Result<PooledConnection, Error> {
        self.0.get().map_err(Error::Pool)
    }

    /// The underlying r2d2 pool, for knobs not surfaced here.
    pub fn inner(&self) -> &diesel::r2d2::Pool<DualConnectionManager> {
        &self.0
    }
}

/// Builder for [`Pool`]. Proxies the common r2d2 settings; for anything else,
/// build a `diesel::r2d2::Pool` directly over [`DualConnectionManager`].
pub struct Builder {
    inner: diesel::r2d2::Builder<DualConnectionManager>,
}

impl Builder {
    /// Maximum number of pooled connections.
    pub fn max_size(mut self, n: u32) -> Self {
        self.inner = self.inner.max_size(n);
        self
    }

    /// Minimum idle connections to maintain.
    pub fn min_idle(mut self, n: Option<u32>) -> Self {
        self.inner = self.inner.min_idle(n);
        self
    }

    /// Maximum time to wait for a connection on checkout before erroring.
    pub fn connection_timeout(mut self, t: std::time::Duration) -> Self {
        self.inner = self.inner.connection_timeout(t);
        self
    }

    /// Detect the backend from `url` and build the pool, eagerly opening the
    /// initial connections (fails if the database is unreachable).
    pub fn connect(self, url: &str) -> Result<Pool, Error> {
        let manager = DualConnectionManager::new(url)?;
        self.inner.build(manager).map(Pool).map_err(Error::Pool)
    }

    /// Detect the backend from `url` and build the pool **lazily**: no
    /// connection is opened at construction, so this never fails just because
    /// the database is not yet reachable — connection errors surface on first
    /// checkout instead. Useful when a service builds its pool before the
    /// database is guaranteed up (e.g. a daemon started alongside its DB).
    pub fn connect_lazy(self, url: &str) -> Result<Pool, Error> {
        let manager = DualConnectionManager::new(url)?;
        Ok(Pool(self.inner.build_unchecked(manager)))
    }
}

/// Errors from [`Pool`] construction and checkout.
#[derive(Debug)]
pub enum Error {
    /// The URL's scheme didn't match a known backend.
    UnknownUrl(String),
    /// An r2d2 build/checkout error (includes the underlying connection error).
    Pool(PoolError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnknownUrl(url) => {
                write!(f, "unrecognized database URL (no known backend): {url}")
            }
            Error::Pool(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::UnknownUrl(_) => None,
            Error::Pool(e) => Some(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_postgres() {
        assert_eq!(detect_backend("postgres://u@h/db"), Some(Backend::Postgres));
        assert_eq!(
            detect_backend("postgresql://u@h:5432/db"),
            Some(Backend::Postgres)
        );
    }

    #[test]
    fn detects_sqlite() {
        for url in [
            ":memory:",
            "file:app.db",
            "file::memory:?cache=shared",
            "sqlite://app.db",
            "app.db",
            "/var/lib/app.sqlite",
            "./data/x.db",
        ] {
            assert_eq!(detect_backend(url), Some(Backend::Sqlite), "url: {url}");
        }
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert_eq!(detect_backend("mysql://h/db"), None);
        assert_eq!(detect_backend("redis://h"), None);
        assert!(DualConnectionManager::new("mysql://h/db").is_err());
    }
}
