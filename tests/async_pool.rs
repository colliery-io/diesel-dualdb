//! `AsyncPool`: deadpool over `AsyncDualConnection`, with backend detection.
//! A pooled connection drives the bridge (insert + filtered select on a
//! portable uuid column) the same as a direct one.
//!
//! SQLite runs in-memory; Postgres runs only when `DUALDB_PG_URL` is set.
#![cfg(all(feature = "async", feature = "uuid"))]

use std::time::Duration;

use diesel::prelude::*;
use diesel_async::{RunQueryDsl, SimpleAsyncConnection};
use diesel_dualdb::async_pool::Error;
use diesel_dualdb::types::Uuid;
use diesel_dualdb::{AsyncDualConnection, AsyncPool};

diesel::table! {
    use diesel::sql_types::Text;
    use diesel_dualdb::sql_types::Uuid;

    async_pooled_items (id) {
        id -> Uuid,
        name -> Text,
    }
}

async fn bridge_via_pooled(conn: &mut AsyncDualConnection) {
    let ddl = match conn {
        AsyncDualConnection::Pg(_) => {
            "CREATE TEMP TABLE async_pooled_items (id UUID PRIMARY KEY NOT NULL, name TEXT NOT NULL);"
        }
        AsyncDualConnection::Sqlite(_) => {
            "CREATE TABLE async_pooled_items (id BLOB PRIMARY KEY NOT NULL, name TEXT NOT NULL);"
        }
    };
    conn.batch_execute(ddl).await.expect("create table");

    let id = Uuid(uuid::Uuid::new_v4());
    diesel::insert_into(async_pooled_items::table)
        .values((
            async_pooled_items::id.eq(id),
            async_pooled_items::name.eq("pooled"),
        ))
        .execute(conn)
        .await
        .expect("insert via pooled connection");
    let got: Uuid = async_pooled_items::table
        .select(async_pooled_items::id)
        .filter(async_pooled_items::id.eq(id))
        .first(conn)
        .await
        .expect("select via pooled connection");
    assert_eq!(got, id);
}

#[tokio::test]
async fn async_pool_sqlite() {
    // max_size 1: a :memory: db is per-connection, so keep it to one.
    let pool = AsyncPool::builder()
        .max_size(1)
        .connect("sqlite://:memory:")
        .expect("build sqlite pool");
    let mut conn = pool.get().await.expect("checkout");
    assert!(matches!(*conn, AsyncDualConnection::Sqlite(_)));
    bridge_via_pooled(&mut conn).await;
}

#[tokio::test]
async fn async_pool_postgres() {
    let Ok(url) = std::env::var("DUALDB_PG_URL") else {
        eprintln!("DUALDB_PG_URL not set — skipping async_pool_postgres");
        return;
    };
    let pool = AsyncPool::builder()
        .max_size(2)
        .connection_timeout(Duration::from_secs(10))
        .connect(&url)
        .expect("build postgres pool");

    let mut conn = pool.get().await.expect("checkout");
    assert!(matches!(*conn, AsyncDualConnection::Pg(_)));
    bridge_via_pooled(&mut conn).await;
    drop(conn);

    // Two concurrent checkouts on a pool of two.
    let (a, b) = tokio::join!(pool.get(), pool.get());
    let (a, b) = (
        a.expect("first concurrent checkout"),
        b.expect("second concurrent checkout"),
    );
    assert!(matches!(*a, AsyncDualConnection::Pg(_)) && matches!(*b, AsyncDualConnection::Pg(_)));
}

#[test]
fn async_pool_rejects_unknown_scheme() {
    assert!(matches!(
        AsyncPool::connect("mysql://localhost/app"),
        Err(Error::UnknownUrl(_))
    ));
}

#[tokio::test]
async fn async_pool_unreachable_errors_on_checkout() {
    // Construction succeeds (no connection opened); the checkout fails.
    let pool = AsyncPool::builder()
        .connection_timeout(Duration::from_secs(5))
        .connect("postgres://nobody@127.0.0.1:1/none")
        .expect("lazy build succeeds");
    assert!(pool.get().await.is_err());
}
