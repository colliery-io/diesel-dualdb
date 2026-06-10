//! `types::Bytes` round-trips on both backends, driven by
//! `#[diesel_dualdb::test]` — one body, one test per backend (through the
//! bridge / `DualConnection`). Migrated from the hand-written 4-way macro in
//! DDB-T-0012.
//!
//! SQLite runs in-memory; Postgres runs only when `DUALDB_PG_URL` is set.

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_dualdb::types::Bytes;
use diesel_dualdb::DualConnection;

diesel::table! {
    use diesel::sql_types::{Integer, Nullable};
    use diesel_dualdb::sql_types::Bytes;

    bytes_items (id) {
        id -> Integer,
        data -> Bytes,
        maybe -> Nullable<Bytes>,
    }
}

const CREATE_SQLITE: &str = "CREATE TABLE bytes_items (\
    id INTEGER PRIMARY KEY NOT NULL, data BLOB NOT NULL, maybe BLOB);";
const CREATE_PG: &str = "CREATE TEMP TABLE bytes_items (\
    id INTEGER PRIMARY KEY NOT NULL, data BYTEA NOT NULL, maybe BYTEA);";

/// The schema is the one legitimately per-backend bit (BLOB vs BYTEA); match on
/// the arm just to pick DDL — the queries below are backend-agnostic.
fn create_table(conn: &mut DualConnection) {
    let ddl = match &*conn {
        DualConnection::Pg(_) => CREATE_PG,
        DualConnection::Sqlite(_) => CREATE_SQLITE,
    };
    conn.batch_execute(ddl).expect("create table");
}

#[diesel_dualdb::test(pg, sqlite)]
fn bytes_round_trips(conn: &mut DualConnection) {
    create_table(conn);

    // Round-trip: empty, mixed-byte (incl. NUL and 0xFF), and 1 MiB.
    let cases: Vec<Vec<u8>> = vec![
        vec![],
        vec![0u8, 255, 0, 1, 2, 254, 128],
        vec![7u8; 1024 * 1024],
    ];
    for (i, raw) in cases.iter().enumerate() {
        let data = Bytes(raw.clone());
        diesel::insert_into(bytes_items::table)
            .values((
                bytes_items::id.eq(i as i32),
                bytes_items::data.eq(data.clone()),
                bytes_items::maybe.eq(None::<Bytes>),
            ))
            .execute(conn)
            .expect("insert data row");
        let got: Bytes = bytes_items::table
            .select(bytes_items::data)
            .filter(bytes_items::id.eq(i as i32))
            .first(conn)
            .expect("select data");
        assert_eq!(got, data, "bytes round-trip, case {i}");
    }
}

#[diesel_dualdb::test(pg, sqlite)]
fn bytes_empty_vs_null(conn: &mut DualConnection) {
    create_table(conn);

    // Some(empty) must not collapse to None.
    diesel::insert_into(bytes_items::table)
        .values((
            bytes_items::id.eq(100),
            bytes_items::data.eq(Bytes(vec![1])),
            bytes_items::maybe.eq(Some(Bytes(vec![]))),
        ))
        .execute(conn)
        .expect("insert empty maybe");
    diesel::insert_into(bytes_items::table)
        .values((
            bytes_items::id.eq(101),
            bytes_items::data.eq(Bytes(vec![1])),
            bytes_items::maybe.eq(None::<Bytes>),
        ))
        .execute(conn)
        .expect("insert null maybe");

    let empty: Option<Bytes> = bytes_items::table
        .select(bytes_items::maybe)
        .filter(bytes_items::id.eq(100))
        .first(conn)
        .expect("select empty maybe");
    let null: Option<Bytes> = bytes_items::table
        .select(bytes_items::maybe)
        .filter(bytes_items::id.eq(101))
        .first(conn)
        .expect("select null maybe");
    assert_eq!(
        empty,
        Some(Bytes(vec![])),
        "Some(empty blob) stays Some(empty)"
    );
    assert_eq!(null, None, "NULL reads back as None");
}
