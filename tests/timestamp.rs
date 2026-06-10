//! `types::Timestamp` through `DualConnection`, driven by
//! `#[diesel_dualdb::test]`. Asserts round-trip, the ADR DDB-A-0001 ordering
//! guarantee (SQLite text sorts chronologically), and microsecond truncation.
//!
//! SQLite runs in-memory; Postgres runs only when `DUALDB_PG_URL` is set.
#![cfg(feature = "chrono")]

use chrono::{DateTime, Duration, TimeZone, Utc};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_dualdb::types::Timestamp;
use diesel_dualdb::DualConnection;

diesel::table! {
    use diesel::sql_types::Integer;
    use diesel_dualdb::sql_types::Timestamp;

    ts_items (id) {
        id -> Integer,
        at -> Timestamp,
    }
}

fn create_table(conn: &mut DualConnection) {
    let ddl = match &*conn {
        DualConnection::Pg(_) => {
            "CREATE TEMP TABLE ts_items (\
            id INTEGER PRIMARY KEY NOT NULL, at TIMESTAMPTZ NOT NULL);"
        }
        DualConnection::Sqlite(_) => {
            "CREATE TABLE ts_items (\
            id INTEGER PRIMARY KEY NOT NULL, at TEXT NOT NULL);"
        }
    };
    conn.batch_execute(ddl).expect("create table");
}

// Microsecond-precision values (exact round-trip on both backends), spanning
// epoch, pre-1970, a sub-second value, and the far future.
fn cases() -> Vec<DateTime<Utc>> {
    vec![
        Utc.with_ymd_and_hms(2026, 6, 8, 15, 4, 5).unwrap() + Duration::microseconds(123_456),
        DateTime::from_timestamp(0, 0).unwrap(),
        Utc.with_ymd_and_hms(1950, 3, 14, 9, 26, 53).unwrap(),
        Utc.with_ymd_and_hms(2999, 12, 31, 23, 59, 59).unwrap(),
    ]
}

#[diesel_dualdb::test(pg, sqlite)]
fn timestamp_round_trips(conn: &mut DualConnection) {
    create_table(conn);

    for (i, dt) in cases().into_iter().enumerate() {
        let at = Timestamp(dt);
        diesel::insert_into(ts_items::table)
            .values((ts_items::id.eq(i as i32), ts_items::at.eq(at)))
            .execute(conn)
            .expect("insert ts");
        let got: Timestamp = ts_items::table
            .select(ts_items::at)
            .filter(ts_items::id.eq(i as i32))
            .first(conn)
            .expect("select ts");
        assert_eq!(got, at, "timestamp round-trip, case {i}");
    }

    // Ordering guarantee: rows inserted out of chronological order come back
    // chronological when ordered by the timestamp column.
    let ordered: Vec<Timestamp> = ts_items::table
        .select(ts_items::at)
        .order(ts_items::at.asc())
        .load(conn)
        .expect("ordered load");
    let mut expected = cases();
    expected.sort();
    let expected: Vec<Timestamp> = expected.into_iter().map(Timestamp).collect();
    assert_eq!(ordered, expected, "ORDER BY at is chronological");
}

#[diesel_dualdb::test(pg, sqlite)]
fn timestamp_truncates_to_micros(conn: &mut DualConnection) {
    create_table(conn);

    let nanos =
        Utc.with_ymd_and_hms(2010, 1, 1, 0, 0, 0).unwrap() + Duration::nanoseconds(123_456_789);
    let truncated =
        Utc.with_ymd_and_hms(2010, 1, 1, 0, 0, 0).unwrap() + Duration::microseconds(123_456);
    diesel::insert_into(ts_items::table)
        .values((ts_items::id.eq(500), ts_items::at.eq(Timestamp(nanos))))
        .execute(conn)
        .expect("insert nanos");
    let got: Timestamp = ts_items::table
        .select(ts_items::at)
        .filter(ts_items::id.eq(500))
        .first(conn)
        .expect("select nanos");
    assert_eq!(
        got,
        Timestamp(truncated),
        "sub-microsecond truncates to micros"
    );
}
