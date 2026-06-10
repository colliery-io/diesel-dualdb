//! Upsert (`ON CONFLICT`) is an **escape-hatch** case.
//!
//! Both Postgres and SQLite support `ON CONFLICT … DO UPDATE / DO NOTHING`, but
//! diesel's `MultiConnection` derive sets `MultiBackend`'s on-conflict dialect
//! to `DoesNotSupportOnConflictClause` — and, unlike `RETURNING`, there's no
//! feature to enable it. So `on_conflict` can't go through `DualConnection` on
//! one arm; you `match conn` and run it on the concrete connection (the query
//! itself is identical on both, so a macro keeps it DRY).
//!
//! This test proves the per-backend path works on both.

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_dualdb::DualConnection;

diesel::table! {
    kv (k) {
        k -> Text,
        v -> Integer,
    }
}

fn create_table(conn: &mut DualConnection) {
    let ddl = match &*conn {
        DualConnection::Pg(_) => {
            "CREATE TEMP TABLE kv (k TEXT PRIMARY KEY NOT NULL, v INTEGER NOT NULL);"
        }
        DualConnection::Sqlite(_) => {
            "CREATE TABLE kv (k TEXT PRIMARY KEY NOT NULL, v INTEGER NOT NULL);"
        }
    };
    conn.batch_execute(ddl).expect("create table");
}

fn value(conn: &mut DualConnection) -> i32 {
    kv::table
        .select(kv::v)
        .filter(kv::k.eq("a"))
        .first(conn)
        .expect("select v")
}

// The upsert query — identical on both backends; `$c` is a concrete connection.
macro_rules! upsert_set_v {
    ($c:expr, $v:expr) => {
        diesel::insert_into(kv::table)
            .values((kv::k.eq("a"), kv::v.eq($v)))
            .on_conflict(kv::k)
            .do_update()
            .set(kv::v.eq($v))
            .execute($c)
            .expect("upsert do_update")
    };
}

macro_rules! upsert_do_nothing {
    ($c:expr, $v:expr) => {
        diesel::insert_into(kv::table)
            .values((kv::k.eq("a"), kv::v.eq($v)))
            .on_conflict(kv::k)
            .do_nothing()
            .execute($c)
            .expect("upsert do_nothing")
    };
}

#[diesel_dualdb::test(pg, sqlite)]
fn upsert_via_escape_hatch(conn: &mut DualConnection) {
    create_table(conn);

    // A plain insert goes through DualConnection (no on_conflict).
    diesel::insert_into(kv::table)
        .values((kv::k.eq("a"), kv::v.eq(1)))
        .execute(conn)
        .expect("initial insert");
    assert_eq!(value(conn), 1);

    // ON CONFLICT DO UPDATE — escape hatch.
    match conn {
        DualConnection::Pg(c) => {
            upsert_set_v!(c, 2);
        }
        DualConnection::Sqlite(c) => {
            upsert_set_v!(c, 2);
        }
    }
    assert_eq!(value(conn), 2, "DO UPDATE applied");

    // ON CONFLICT DO NOTHING — escape hatch.
    match conn {
        DualConnection::Pg(c) => {
            upsert_do_nothing!(c, 99);
        }
        DualConnection::Sqlite(c) => {
            upsert_do_nothing!(c, 99);
        }
    }
    assert_eq!(value(conn), 2, "DO NOTHING left the row unchanged");
}

/// The same upsert, written with the `dispatch` escape-hatch helper instead of a
/// bare `match` — both arms still required, just tidier.
#[diesel_dualdb::test(pg, sqlite)]
fn upsert_via_dispatch(conn: &mut DualConnection) {
    create_table(conn);
    diesel::insert_into(kv::table)
        .values((kv::k.eq("a"), kv::v.eq(1)))
        .execute(conn)
        .expect("initial insert");

    conn.dispatch(|pg| upsert_set_v!(pg, 7), |sqlite| upsert_set_v!(sqlite, 7));
    assert_eq!(value(conn), 7, "dispatch upsert applied");
}
