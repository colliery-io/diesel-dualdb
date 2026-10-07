//! `types::Uuid` through `AsyncDualConnection` (the DDB-T-0027 spike): the
//! same write-once query as `tests/uuid.rs`, run async on both arms.
//!
//! SQLite runs in-memory; Postgres runs only when `DUALDB_PG_URL` is set.
#![cfg(all(feature = "async", feature = "uuid"))]

use diesel::prelude::*;
use diesel_async::{AsyncConnection, RunQueryDsl, SimpleAsyncConnection};
use diesel_dualdb::types::Uuid;
use diesel_dualdb::AsyncDualConnection;

diesel::table! {
    use diesel::sql_types::{Integer, Text};
    use diesel_dualdb::sql_types::Uuid;

    uuid_items (id) {
        id -> Uuid,
        name -> Text,
        count -> Integer,
    }
}

#[derive(Queryable, Insertable, Selectable, PartialEq, Debug, Clone)]
#[diesel(table_name = uuid_items)]
struct Item {
    id: Uuid,
    name: String,
    count: i32,
}

async fn create_table(conn: &mut AsyncDualConnection) {
    let ddl = match conn {
        AsyncDualConnection::Pg(_) => {
            "CREATE TEMP TABLE uuid_items (\
            id UUID PRIMARY KEY NOT NULL, name TEXT NOT NULL, count INTEGER NOT NULL);"
        }
        AsyncDualConnection::Sqlite(_) => {
            "CREATE TABLE uuid_items (\
            id BLOB PRIMARY KEY NOT NULL, name TEXT NOT NULL, count INTEGER NOT NULL);"
        }
    };
    conn.batch_execute(ddl).await.expect("create table");
}

/// The write-once body: no per-backend code past the DDL.
async fn round_trip(conn: &mut AsyncDualConnection) {
    create_table(conn).await;

    let item = Item {
        id: Uuid(uuid::Uuid::new_v4()),
        name: "widget".to_owned(),
        count: 42,
    };

    // insert ... RETURNING *
    let inserted: Item = diesel::insert_into(uuid_items::table)
        .values(&item)
        .get_result(conn)
        .await
        .expect("insert with RETURNING");
    assert_eq!(inserted, item, "RETURNING row matches");

    // plain execute
    let n = diesel::insert_into(uuid_items::table)
        .values((
            uuid_items::id.eq(Uuid(uuid::Uuid::nil())),
            uuid_items::name.eq("nil"),
            uuid_items::count.eq(0),
        ))
        .execute(conn)
        .await
        .expect("insert");
    assert_eq!(n, 1);

    // filter on a bound Uuid
    let found: Item = uuid_items::table
        .filter(uuid_items::id.eq(item.id))
        .select(Item::as_select())
        .first(conn)
        .await
        .expect("select by uuid");
    assert_eq!(found, item, "round-tripped row matches");

    let all: Vec<(Uuid, String)> = uuid_items::table
        .select((uuid_items::id, uuid_items::name))
        .order(uuid_items::count)
        .load(conn)
        .await
        .expect("load all");
    assert_eq!(
        all,
        vec![
            (Uuid(uuid::Uuid::nil()), "nil".to_owned()),
            (item.id, "widget".to_owned())
        ]
    );
}

#[tokio::test]
async fn async_uuid_round_trip_sqlite() {
    let mut conn = AsyncDualConnection::establish(":memory:")
        .await
        .expect("open sqlite");
    assert!(matches!(conn, AsyncDualConnection::Sqlite(_)));
    round_trip(&mut conn).await;
}

#[tokio::test]
async fn async_uuid_round_trip_pg() {
    let Ok(url) = std::env::var("DUALDB_PG_URL") else {
        eprintln!("DUALDB_PG_URL not set — skipping async_uuid_round_trip_pg");
        return;
    };
    let mut conn = AsyncDualConnection::establish(&url)
        .await
        .expect("connect to postgres");
    assert!(matches!(conn, AsyncDualConnection::Pg(_)));
    round_trip(&mut conn).await;
}
