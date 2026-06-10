//! `types::Uuid` through `DualConnection`, driven by `#[diesel_dualdb::test]`.
//! Consolidates the old `tests/types.rs` (round-trip) and `tests/bridge.rs`
//! (insert-RETURNING + filter) onto the macro.
//!
//! SQLite runs in-memory; Postgres runs only when `DUALDB_PG_URL` is set.
#![cfg(feature = "uuid")]

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_dualdb::types::Uuid;
use diesel_dualdb::DualConnection;

diesel::table! {
    use diesel::sql_types::{Integer, Text};
    use diesel_dualdb::sql_types::Uuid;

    // A mixed row: a portable Uuid PK alongside native Text + Integer.
    uuid_items (id) {
        id -> Uuid,
        name -> Text,
        count -> Integer,
    }
}

#[derive(Queryable, Insertable, PartialEq, Debug, Clone)]
#[diesel(table_name = uuid_items)]
struct Item {
    id: Uuid,
    name: String,
    count: i32,
}

/// Per-backend DDL (UUID vs BLOB) — the one legitimately divergent bit.
fn create_table(conn: &mut DualConnection) {
    let ddl = match &*conn {
        DualConnection::Pg(_) => {
            "CREATE TEMP TABLE uuid_items (\
            id UUID PRIMARY KEY NOT NULL, name TEXT NOT NULL, count INTEGER NOT NULL);"
        }
        DualConnection::Sqlite(_) => {
            "CREATE TABLE uuid_items (\
            id BLOB PRIMARY KEY NOT NULL, name TEXT NOT NULL, count INTEGER NOT NULL);"
        }
    };
    conn.batch_execute(ddl).expect("create table");
}

#[diesel_dualdb::test(pg, sqlite)]
fn uuid_round_trips(conn: &mut DualConnection) {
    create_table(conn);

    for (i, raw) in [uuid::Uuid::new_v4(), uuid::Uuid::nil(), uuid::Uuid::max()]
        .into_iter()
        .enumerate()
    {
        let id = Uuid(raw);
        let name = format!("item-{i}");
        diesel::insert_into(uuid_items::table)
            .values((
                uuid_items::id.eq(id),
                uuid_items::name.eq(&name),
                uuid_items::count.eq(i as i32),
            ))
            .execute(conn)
            .expect("insert row");

        let (got_id, got_name, got_count): (Uuid, String, i32) = uuid_items::table
            .filter(uuid_items::id.eq(id))
            .select((uuid_items::id, uuid_items::name, uuid_items::count))
            .first(conn)
            .expect("select row back");
        assert_eq!(got_id, id, "uuid round-trip (case {i})");
        assert_eq!(got_name, name);
        assert_eq!(got_count, i as i32);
    }
}

#[diesel_dualdb::test(pg, sqlite)]
fn uuid_insert_returning(conn: &mut DualConnection) {
    create_table(conn);

    let item = Item {
        id: Uuid(uuid::Uuid::new_v4()),
        name: "widget".to_owned(),
        count: 42,
    };

    // insert ... RETURNING * — works on both backends through the bridge.
    let inserted: Item = diesel::insert_into(uuid_items::table)
        .values(&item)
        .get_result(conn)
        .expect("insert with RETURNING");
    assert_eq!(inserted, item, "RETURNING row matches");

    let found: Item = uuid_items::table
        .filter(uuid_items::id.eq(item.id))
        .get_result(conn)
        .expect("select by uuid");
    assert_eq!(found, item, "round-tripped row matches");
}
