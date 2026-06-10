//! `Array` round-trips on both backends (PG native `T[]`, SQLite JSON TEXT).
#![cfg(feature = "array")]

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_dualdb::types::Array;
use diesel_dualdb::DualConnection;

diesel::table! {
    use diesel::sql_types::{Integer, Text};
    use diesel_dualdb::sql_types::Array;

    arr_items (id) {
        id -> Integer,
        tags -> Array<Text>,
        nums -> Array<Integer>,
    }
}

fn create_table(conn: &mut DualConnection) {
    let ddl = match &*conn {
        DualConnection::Pg(_) => {
            "CREATE TEMP TABLE arr_items (\
                id INTEGER PRIMARY KEY NOT NULL, \
                tags TEXT[] NOT NULL, \
                nums INTEGER[] NOT NULL);"
        }
        DualConnection::Sqlite(_) => {
            "CREATE TABLE arr_items (\
                id INTEGER PRIMARY KEY NOT NULL, \
                tags TEXT NOT NULL, \
                nums TEXT NOT NULL);"
        }
    };
    conn.batch_execute(ddl).expect("create table");
}

#[diesel_dualdb::test(pg, sqlite)]
fn array_round_trips(conn: &mut DualConnection) {
    create_table(conn);

    let tags = Array(vec!["red".to_string(), "blue".to_string()]);
    let nums = Array(vec![1, 2, 3]);

    diesel::insert_into(arr_items::table)
        .values((
            arr_items::id.eq(1),
            arr_items::tags.eq(tags.clone()),
            arr_items::nums.eq(nums.clone()),
        ))
        .execute(conn)
        .expect("insert arrays");

    let (got_tags, got_nums): (Array<String>, Array<i32>) = arr_items::table
        .select((arr_items::tags, arr_items::nums))
        .filter(arr_items::id.eq(1))
        .first(conn)
        .expect("select arrays");

    assert_eq!(got_tags, tags);
    assert_eq!(got_nums, nums);

    // empty array round-trips too
    diesel::insert_into(arr_items::table)
        .values((
            arr_items::id.eq(2),
            arr_items::tags.eq(Array(Vec::<String>::new())),
            arr_items::nums.eq(Array(Vec::<i32>::new())),
        ))
        .execute(conn)
        .expect("insert empty arrays");
    let empty: Array<i32> = arr_items::table
        .select(arr_items::nums)
        .filter(arr_items::id.eq(2))
        .first(conn)
        .expect("select empty array");
    assert_eq!(empty, Array(vec![]));
}
