//! Dogfood: prove the schema generator end to end. This test uses the
//! generator's own output — the `schema.rs` it produced **and** the per-backend
//! migration SQL it produced — to create a table and round-trip a row on both
//! backends. If the generator emits anything wrong, this fails to compile or
//! fails at runtime.
//!
//! Requires all type features (the generated schema references every portable
//! marker). SQLite runs in-memory; Postgres runs only when `DUALDB_PG_URL` is set.
#![cfg(all(feature = "uuid", feature = "chrono", feature = "serde_json"))]

use chrono::{TimeZone, Utc};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_dualdb::types::{Bytes, Json, Timestamp, Uuid};
use diesel_dualdb::DualConnection;
use serde_json::json;

// The generated `table!` definitions — exactly what an adopter would commit.
include!("../schema/generated/schema.rs");

// The generated per-backend migration DDL.
const PG_UP: &str =
    include_str!("../schema/generated/migrations-postgres/2026-01-01-000000_init/up.sql");
const SQLITE_UP: &str =
    include_str!("../schema/generated/migrations-sqlite/2026-01-01-000000_init/up.sql");

fn apply_init(conn: &mut DualConnection) {
    let up = match &*conn {
        DualConnection::Pg(_) => PG_UP,
        DualConnection::Sqlite(_) => SQLITE_UP,
    };
    conn.batch_execute(up).expect("run generated migration");
}

#[diesel_dualdb::test(pg, sqlite)]
fn generated_schema_round_trips(conn: &mut DualConnection) {
    apply_init(conn);

    let id = Uuid(uuid::Uuid::new_v4());
    let created = Timestamp(Utc.with_ymd_and_hms(2026, 6, 9, 12, 0, 0).unwrap());
    let meta = Json(json!({"k": "v", "n": 3}));

    diesel::insert_into(widgets::table)
        .values((
            widgets::id.eq(id),
            widgets::name.eq("gizmo"),
            widgets::data.eq(Bytes(vec![1, 2, 3, 255])),
            widgets::meta.eq(Some(meta.clone())),
            widgets::created_at.eq(created),
        ))
        .execute(conn)
        .expect("insert widget");

    let row: (
        Uuid,
        String,
        Bytes,
        Option<Json<serde_json::Value>>,
        Timestamp,
    ) = widgets::table
        .filter(widgets::id.eq(id))
        .select((
            widgets::id,
            widgets::name,
            widgets::data,
            widgets::meta,
            widgets::created_at,
        ))
        .first(conn)
        .expect("select widget");

    assert_eq!(row.0, id);
    assert_eq!(row.1, "gizmo");
    assert_eq!(row.2, Bytes(vec![1, 2, 3, 255]));
    assert_eq!(row.3, Some(meta));
    assert_eq!(row.4, created);
}
