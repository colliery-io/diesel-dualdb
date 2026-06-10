//! `types::Json<T>` through `DualConnection`, driven by
//! `#[diesel_dualdb::test]`. Exercises `serde_json::Value` (unicode, nested,
//! null, empty) and a typed struct.
//!
//! SQLite runs in-memory; Postgres runs only when `DUALDB_PG_URL` is set.
#![cfg(feature = "serde_json")]

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_dualdb::types::Json;
use diesel_dualdb::DualConnection;
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
    count: i64,
}

fn create_table(conn: &mut DualConnection) {
    let ddl = match &*conn {
        DualConnection::Pg(_) => {
            "CREATE TEMP TABLE json_items (\
            id INTEGER PRIMARY KEY NOT NULL, doc JSONB NOT NULL);"
        }
        DualConnection::Sqlite(_) => {
            "CREATE TABLE json_items (\
            id INTEGER PRIMARY KEY NOT NULL, doc TEXT NOT NULL);"
        }
    };
    conn.batch_execute(ddl).expect("create table");
}

#[diesel_dualdb::test(pg, sqlite)]
fn json_value_round_trips(conn: &mut DualConnection) {
    create_table(conn);

    let values: Vec<Value> = vec![
        json!(null),
        json!({}),
        json!([]),
        json!({"name": "naïve café ☕", "nested": {"a": [1, 2, 3], "b": true}, "n": 42}),
        json!([1, "two", null, {"k": "v"}]),
    ];
    for (i, v) in values.into_iter().enumerate() {
        diesel::insert_into(json_items::table)
            .values((
                json_items::id.eq(i as i32),
                json_items::doc.eq(Json(v.clone())),
            ))
            .execute(conn)
            .expect("insert value");
        let got: Json<Value> = json_items::table
            .select(json_items::doc)
            .filter(json_items::id.eq(i as i32))
            .first(conn)
            .expect("select value");
        assert_eq!(got.0, v, "json Value round-trip, case {i}");
    }
}

#[diesel_dualdb::test(pg, sqlite)]
fn json_typed_struct_round_trips(conn: &mut DualConnection) {
    create_table(conn);

    let doc = Doc {
        name: "widget".to_owned(),
        tags: vec!["a".to_owned(), "b".to_owned()],
        count: 7,
    };
    diesel::insert_into(json_items::table)
        .values((json_items::id.eq(1), json_items::doc.eq(Json(doc.clone()))))
        .execute(conn)
        .expect("insert struct");
    let got: Json<Doc> = json_items::table
        .select(json_items::doc)
        .filter(json_items::id.eq(1))
        .first(conn)
        .expect("select struct");
    assert_eq!(got.0, doc, "typed struct round-trip");
}
