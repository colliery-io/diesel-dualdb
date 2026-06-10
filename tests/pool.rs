//! Pooling check: r2d2 over `DualConnection`.
//!
//! The `MultiConnection` derive provides `R2D2Connection` for the enum, so
//! diesel's `ConnectionManager<DualConnection>` + `r2d2::Pool` work directly —
//! and a pooled connection drives the bridge (insert + filtered select on a
//! portable uuid column) the same as a direct one.
//!
//! The raw-`ConnectionManager` tests below lean on the derive's `establish`
//! (Pg-then-Sqlite fallthrough). The `dualdb::Pool::connect` tests use the
//! explicit URL/scheme detection added in M2.
#![cfg(feature = "uuid")]

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel_dualdb::types::Uuid;
use diesel_dualdb::DualConnection;

diesel::table! {
    use diesel::sql_types::Text;
    use diesel_dualdb::sql_types::Uuid;

    pooled_items (id) {
        id -> Uuid,
        name -> Text,
    }
}

fn bridge_via_pooled(conn: &mut DualConnection, create_sql: &str) {
    conn.batch_execute(create_sql).expect("create table");
    let id = Uuid(uuid::Uuid::new_v4());
    diesel::insert_into(pooled_items::table)
        .values((pooled_items::id.eq(id), pooled_items::name.eq("pooled")))
        .execute(conn)
        .expect("insert via pooled connection");
    let got: Uuid = pooled_items::table
        .select(pooled_items::id)
        .filter(pooled_items::id.eq(id))
        .first(conn)
        .expect("select via pooled connection");
    assert_eq!(got, id);
}

#[test]
fn r2d2_pool_over_dualconnection_sqlite() {
    // max_size 1: a :memory: db is per-connection, so keep it to one.
    let manager = ConnectionManager::<DualConnection>::new(":memory:");
    let pool = Pool::builder()
        .max_size(1)
        .build(manager)
        .expect("build sqlite pool");
    let mut conn = pool.get().expect("checkout pooled DualConnection");
    bridge_via_pooled(
        &mut conn,
        "CREATE TABLE pooled_items (id BLOB PRIMARY KEY NOT NULL, name TEXT NOT NULL);",
    );
}

#[test]
fn r2d2_pool_over_dualconnection_postgres() {
    let Ok(url) = std::env::var("DUALDB_PG_URL") else {
        eprintln!("DUALDB_PG_URL not set — skipping Postgres pool test");
        return;
    };
    let manager = ConnectionManager::<DualConnection>::new(&url);
    let pool = Pool::builder()
        .max_size(2)
        .build(manager)
        .expect("build postgres pool");
    let mut conn = pool.get().expect("checkout pooled DualConnection");
    bridge_via_pooled(
        &mut conn,
        "CREATE TEMP TABLE pooled_items (id UUID PRIMARY KEY NOT NULL, name TEXT NOT NULL);",
    );
}

#[test]
fn dualdb_pool_connect_sqlite() {
    // `:memory:` is detected as SQLite; max_size 1 keeps it to one db.
    let pool = diesel_dualdb::Pool::builder()
        .max_size(1)
        .connect(":memory:")
        .expect("connect sqlite pool");
    let mut conn = pool.get().expect("checkout");
    bridge_via_pooled(
        &mut conn,
        "CREATE TABLE pooled_items (id BLOB PRIMARY KEY NOT NULL, name TEXT NOT NULL);",
    );
}

#[test]
fn dualdb_pool_connect_postgres() {
    let Ok(url) = std::env::var("DUALDB_PG_URL") else {
        eprintln!("DUALDB_PG_URL not set — skipping Postgres pool test");
        return;
    };
    // `postgres://…` is detected as Postgres — no Pg-then-Sqlite fallthrough.
    let pool = diesel_dualdb::Pool::connect(&url).expect("connect postgres pool");
    let mut conn = pool.get().expect("checkout");
    bridge_via_pooled(
        &mut conn,
        "CREATE TEMP TABLE pooled_items (id UUID PRIMARY KEY NOT NULL, name TEXT NOT NULL);",
    );
}
