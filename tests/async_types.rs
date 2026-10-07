//! Every portable type through `AsyncDualConnection`, driven by
//! `#[diesel_dualdb::test]` on `async fn`s: the same write-once queries as the
//! sync per-type tests, awaited, on both arms.
//!
//! SQLite runs in-memory; Postgres runs only when `DUALDB_PG_URL` is set.
#![cfg(feature = "async")]

use diesel::prelude::*;
use diesel_async::{RunQueryDsl, SimpleAsyncConnection};
use diesel_dualdb::AsyncDualConnection;

/// Run the DDL for the active arm — the one legitimately divergent bit.
async fn ddl(conn: &mut AsyncDualConnection, pg: &str, sqlite: &str) {
    let sql = match conn {
        AsyncDualConnection::Pg(_) => pg,
        AsyncDualConnection::Sqlite(_) => sqlite,
    };
    conn.batch_execute(sql).await.expect("create schema");
}

#[cfg(feature = "uuid")]
mod uuid_type {
    use super::*;
    use diesel_dualdb::types::Uuid;

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

    #[diesel_dualdb::test(pg, sqlite)]
    async fn async_uuid_round_trips(conn: &mut AsyncDualConnection) {
        ddl(
            conn,
            "CREATE TEMP TABLE uuid_items (\
             id UUID PRIMARY KEY NOT NULL, name TEXT NOT NULL, count INTEGER NOT NULL);",
            "CREATE TABLE uuid_items (\
             id BLOB PRIMARY KEY NOT NULL, name TEXT NOT NULL, count INTEGER NOT NULL);",
        )
        .await;

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
}

mod bytes_type {
    use super::*;
    use diesel_dualdb::types::Bytes;

    diesel::table! {
        use diesel::sql_types::{Integer, Nullable};
        use diesel_dualdb::sql_types::Bytes;

        bytes_items (id) {
            id -> Integer,
            data -> Bytes,
            maybe -> Nullable<Bytes>,
        }
    }

    #[diesel_dualdb::test(pg, sqlite)]
    async fn async_bytes_round_trips(conn: &mut AsyncDualConnection) {
        ddl(
            conn,
            "CREATE TEMP TABLE bytes_items (\
             id INTEGER PRIMARY KEY NOT NULL, data BYTEA NOT NULL, maybe BYTEA);",
            "CREATE TABLE bytes_items (\
             id INTEGER PRIMARY KEY NOT NULL, data BLOB NOT NULL, maybe BLOB);",
        )
        .await;

        let cases: Vec<(Vec<u8>, Option<Vec<u8>>)> = vec![
            (vec![], Some(vec![])),
            (vec![0u8, 255, 0, 1, 2, 254, 128], None),
            (vec![7u8; 1024 * 1024], Some(vec![9])),
        ];
        for (i, (data, maybe)) in cases.iter().enumerate() {
            diesel::insert_into(bytes_items::table)
                .values((
                    bytes_items::id.eq(i as i32),
                    bytes_items::data.eq(Bytes(data.clone())),
                    bytes_items::maybe.eq(maybe.clone().map(Bytes)),
                ))
                .execute(conn)
                .await
                .expect("insert bytes");

            let (got, got_maybe): (Bytes, Option<Bytes>) = bytes_items::table
                .select((bytes_items::data, bytes_items::maybe))
                .filter(bytes_items::id.eq(i as i32))
                .first(conn)
                .await
                .expect("select bytes");
            assert_eq!(got, Bytes(data.clone()), "bytes, case {i}");
            assert_eq!(
                got_maybe,
                maybe.clone().map(Bytes),
                "empty vs NULL, case {i}"
            );
        }
    }
}

#[cfg(feature = "chrono")]
mod timestamp_type {
    use super::*;
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use diesel_dualdb::types::Timestamp;

    diesel::table! {
        use diesel::sql_types::Integer;
        use diesel_dualdb::sql_types::Timestamp;

        ts_items (id) {
            id -> Integer,
            at -> Timestamp,
        }
    }

    #[diesel_dualdb::test(pg, sqlite)]
    async fn async_timestamp_round_trips(conn: &mut AsyncDualConnection) {
        ddl(
            conn,
            "CREATE TEMP TABLE ts_items (id INTEGER PRIMARY KEY NOT NULL, at TIMESTAMPTZ NOT NULL);",
            "CREATE TABLE ts_items (id INTEGER PRIMARY KEY NOT NULL, at TEXT NOT NULL);",
        )
        .await;

        let mut cases: Vec<DateTime<Utc>> = vec![
            Utc.with_ymd_and_hms(2026, 6, 8, 15, 4, 5).unwrap() + Duration::microseconds(123_456),
            DateTime::from_timestamp(0, 0).unwrap(),
            Utc.with_ymd_and_hms(1950, 3, 14, 9, 26, 53).unwrap(),
        ];
        for (i, dt) in cases.iter().enumerate() {
            diesel::insert_into(ts_items::table)
                .values((ts_items::id.eq(i as i32), ts_items::at.eq(Timestamp(*dt))))
                .execute(conn)
                .await
                .expect("insert ts");
        }

        let ordered: Vec<Timestamp> = ts_items::table
            .select(ts_items::at)
            .order(ts_items::at.asc())
            .load(conn)
            .await
            .expect("ordered load");
        cases.sort();
        assert_eq!(
            ordered,
            cases.into_iter().map(Timestamp).collect::<Vec<_>>()
        );
    }
}

#[cfg(feature = "serde_json")]
mod json_type {
    use super::*;
    use diesel_dualdb::types::Json;
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};

    diesel::table! {
        use diesel::sql_types::Integer;
        use diesel_dualdb::sql_types::Json;

        json_items (id) {
            id -> Integer,
            doc -> Json,
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct Doc {
        name: String,
        tags: Vec<String>,
    }

    #[diesel_dualdb::test(pg, sqlite)]
    async fn async_json_round_trips(conn: &mut AsyncDualConnection) {
        ddl(
            conn,
            "CREATE TEMP TABLE json_items (id INTEGER PRIMARY KEY NOT NULL, doc JSONB NOT NULL);",
            "CREATE TABLE json_items (id INTEGER PRIMARY KEY NOT NULL, doc TEXT NOT NULL);",
        )
        .await;

        let value = json!({"name": "naïve café ☕", "nested": {"a": [1, 2, 3]}, "n": 42});
        diesel::insert_into(json_items::table)
            .values((
                json_items::id.eq(1),
                json_items::doc.eq(Json(value.clone())),
            ))
            .execute(conn)
            .await
            .expect("insert value");
        let got: Json<Value> = json_items::table
            .select(json_items::doc)
            .filter(json_items::id.eq(1))
            .first(conn)
            .await
            .expect("select value");
        assert_eq!(got.0, value);

        let doc = Doc {
            name: "widget".to_owned(),
            tags: vec!["a".to_owned(), "b".to_owned()],
        };
        diesel::insert_into(json_items::table)
            .values((json_items::id.eq(2), json_items::doc.eq(Json(doc.clone()))))
            .execute(conn)
            .await
            .expect("insert struct");
        let got: Json<Doc> = json_items::table
            .select(json_items::doc)
            .filter(json_items::id.eq(2))
            .first(conn)
            .await
            .expect("select struct");
        assert_eq!(got.0, doc);
    }
}

#[cfg(feature = "decimal")]
mod decimal_type {
    use super::*;
    use bigdecimal::BigDecimal;
    use diesel_dualdb::types::Decimal;
    use std::str::FromStr;

    diesel::table! {
        use diesel::sql_types::Integer;
        use diesel_dualdb::sql_types::Decimal;

        money (id) {
            id -> Integer,
            amount -> Decimal,
        }
    }

    #[diesel_dualdb::test(pg, sqlite)]
    async fn async_decimal_round_trips(conn: &mut AsyncDualConnection) {
        ddl(
            conn,
            "CREATE TEMP TABLE money (id INTEGER PRIMARY KEY NOT NULL, amount NUMERIC NOT NULL);",
            "CREATE TABLE money (id INTEGER PRIMARY KEY NOT NULL, amount TEXT NOT NULL);",
        )
        .await;

        for (id, raw) in [(1, "12345.6789"), (2, "-0.000000000001")] {
            let amount = Decimal(BigDecimal::from_str(raw).unwrap());
            diesel::insert_into(money::table)
                .values((money::id.eq(id), money::amount.eq(amount.clone())))
                .execute(conn)
                .await
                .expect("insert decimal");
            let got: Decimal = money::table
                .select(money::amount)
                .filter(money::id.eq(id))
                .first(conn)
                .await
                .expect("select decimal");
            assert_eq!(got, amount);
        }
    }
}

#[cfg(feature = "array")]
mod array_type {
    use super::*;
    use diesel_dualdb::types::Array;

    diesel::table! {
        use diesel::sql_types::{Integer, Text};
        use diesel_dualdb::sql_types::Array;

        arr_items (id) {
            id -> Integer,
            tags -> Array<Text>,
            nums -> Array<Integer>,
        }
    }

    #[diesel_dualdb::test(pg, sqlite)]
    async fn async_array_round_trips(conn: &mut AsyncDualConnection) {
        ddl(
            conn,
            "CREATE TEMP TABLE arr_items (\
             id INTEGER PRIMARY KEY NOT NULL, tags TEXT[] NOT NULL, nums INTEGER[] NOT NULL);",
            "CREATE TABLE arr_items (\
             id INTEGER PRIMARY KEY NOT NULL, tags TEXT NOT NULL, nums TEXT NOT NULL);",
        )
        .await;

        let cases = [
            (
                1,
                Array(vec!["red".to_string(), "blue".to_string()]),
                Array(vec![1, 2, 3]),
            ),
            (2, Array(Vec::<String>::new()), Array(Vec::<i32>::new())),
        ];
        for (id, tags, nums) in cases {
            diesel::insert_into(arr_items::table)
                .values((
                    arr_items::id.eq(id),
                    arr_items::tags.eq(tags.clone()),
                    arr_items::nums.eq(nums.clone()),
                ))
                .execute(conn)
                .await
                .expect("insert arrays");
            let got: (Array<String>, Array<i32>) = arr_items::table
                .select((arr_items::tags, arr_items::nums))
                .filter(arr_items::id.eq(id))
                .first(conn)
                .await
                .expect("select arrays");
            assert_eq!(got, (tags, nums));
        }
    }
}

/// A Postgres native `enum` is the one bridged type whose OID is not fixed:
/// diesel-async resolves it through its own metadata lookup, which reaches the
/// bridge through `ForeignPgLookup`.
mod enum_type {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, diesel_dualdb::DualEnum)]
    #[dualdb(pg_type = "async_mood")]
    pub enum Mood {
        Happy,
        Sad,
        #[dualdb(rename = "meh")]
        Neutral,
    }

    diesel::table! {
        use diesel::sql_types::Integer;
        use super::MoodSqlType;

        async_feelings (id) {
            id -> Integer,
            mood -> MoodSqlType,
        }
    }

    #[diesel_dualdb::test(pg, sqlite)]
    async fn async_enum_round_trips(conn: &mut AsyncDualConnection) {
        ddl(
            conn,
            // The enum type persists in the test database, so setup is idempotent.
            "DROP TYPE IF EXISTS async_mood CASCADE;\
             CREATE TYPE async_mood AS ENUM ('Happy', 'Sad', 'meh');\
             CREATE TEMP TABLE async_feelings (\
             id INTEGER PRIMARY KEY NOT NULL, mood async_mood NOT NULL);",
            "CREATE TABLE async_feelings (\
             id INTEGER PRIMARY KEY NOT NULL, \
             mood TEXT NOT NULL CHECK (mood IN ('Happy', 'Sad', 'meh')));",
        )
        .await;

        for (id, mood) in [(1, Mood::Happy), (2, Mood::Sad), (3, Mood::Neutral)] {
            diesel::insert_into(async_feelings::table)
                .values((async_feelings::id.eq(id), async_feelings::mood.eq(mood)))
                .execute(conn)
                .await
                .expect("insert mood");
        }

        let neutral: Vec<i32> = async_feelings::table
            .select(async_feelings::id)
            .filter(async_feelings::mood.eq(Mood::Neutral))
            .load(conn)
            .await
            .expect("filter on a bound enum");
        assert_eq!(neutral, vec![3]);

        let all: Vec<Mood> = async_feelings::table
            .select(async_feelings::mood)
            .order(async_feelings::id)
            .load(conn)
            .await
            .expect("load all moods");
        assert_eq!(all, vec![Mood::Happy, Mood::Sad, Mood::Neutral]);
    }
}

mod connection {
    use super::*;
    use diesel_async::AsyncConnection;

    diesel::table! {
        counters (id) {
            id -> Integer,
            n -> Integer,
        }
    }

    async fn setup(conn: &mut AsyncDualConnection) {
        ddl(
            conn,
            "CREATE TEMP TABLE counters (id INTEGER PRIMARY KEY NOT NULL, n INTEGER NOT NULL);",
            "CREATE TABLE counters (id INTEGER PRIMARY KEY NOT NULL, n INTEGER NOT NULL);",
        )
        .await;
    }

    async fn count(conn: &mut AsyncDualConnection) -> i64 {
        counters::table
            .count()
            .get_result(conn)
            .await
            .expect("count")
    }

    #[diesel_dualdb::test(pg, sqlite)]
    async fn async_transaction_commits(conn: &mut AsyncDualConnection) {
        setup(conn).await;
        let n = conn
            .transaction(async |c| {
                diesel::insert_into(counters::table)
                    .values((counters::id.eq(1), counters::n.eq(10)))
                    .execute(c)
                    .await
            })
            .await
            .expect("transaction commits");
        assert_eq!(n, 1);
        assert_eq!(count(conn).await, 1);
    }

    #[diesel_dualdb::test(pg, sqlite)]
    async fn async_transaction_rolls_back(conn: &mut AsyncDualConnection) {
        setup(conn).await;
        let result: Result<(), diesel::result::Error> = conn
            .transaction(async |c| {
                diesel::insert_into(counters::table)
                    .values((counters::id.eq(1), counters::n.eq(10)))
                    .execute(c)
                    .await?;
                // Nested: a savepoint that rolls back on its own.
                let inner: Result<(), diesel::result::Error> = c
                    .transaction(async |c| {
                        diesel::insert_into(counters::table)
                            .values((counters::id.eq(2), counters::n.eq(20)))
                            .execute(c)
                            .await?;
                        Err(diesel::result::Error::RollbackTransaction)
                    })
                    .await;
                assert!(inner.is_err());
                assert_eq!(count(c).await, 1, "savepoint rolled back, outer row kept");
                Err(diesel::result::Error::RollbackTransaction)
            })
            .await;
        assert!(result.is_err());
        assert_eq!(count(conn).await, 0, "outer transaction rolled back");
    }

    #[diesel_dualdb::test(pg, sqlite)]
    async fn async_raw_sql_query(conn: &mut AsyncDualConnection) {
        #[derive(QueryableByName, Debug, PartialEq)]
        struct Row {
            #[diesel(sql_type = diesel::sql_types::Integer)]
            x: i32,
            #[diesel(sql_type = diesel::sql_types::Text)]
            s: String,
        }
        let rows: Vec<Row> = diesel::sql_query("SELECT 7 AS x, 'seven' AS s")
            .load(conn)
            .await
            .expect("sql_query");
        assert_eq!(
            rows,
            vec![Row {
                x: 7,
                s: "seven".to_owned()
            }]
        );
    }

    #[tokio::test]
    async fn async_establish_rejects_unknown_scheme() {
        let err = AsyncDualConnection::establish("mysql://localhost/app")
            .await
            .err()
            .expect("mysql:// is not a supported backend");
        assert!(err.to_string().contains("mysql://"), "{err}");
    }

    #[tokio::test]
    async fn async_establish_sqlite_scheme() {
        let conn = AsyncDualConnection::establish("sqlite://:memory:")
            .await
            .expect("sqlite:// URL");
        assert!(matches!(conn, AsyncDualConnection::Sqlite(_)));
    }
}
