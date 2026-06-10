//! Portable enum: a `DualEnum`-derived enum round-trips on both backends
//! (PostgreSQL native `enum`, SQLite `TEXT`).

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_dualdb::DualConnection;

#[derive(Debug, Clone, Copy, PartialEq, diesel_dualdb::DualEnum)]
#[dualdb(pg_type = "mood")]
pub enum Mood {
    Happy,
    Sad,
    #[dualdb(rename = "meh")]
    Neutral,
}

diesel::table! {
    use diesel::sql_types::Integer;
    use super::MoodSqlType;

    feelings (id) {
        id -> Integer,
        mood -> MoodSqlType,
    }
}

fn create_schema(conn: &mut DualConnection) {
    let ddl: &[&str] = match &*conn {
        DualConnection::Pg(_) => &[
            // The enum type persists in the test database, so make setup idempotent.
            "DROP TYPE IF EXISTS mood CASCADE;",
            "CREATE TYPE mood AS ENUM ('Happy', 'Sad', 'meh');",
            "CREATE TEMP TABLE feelings (id INTEGER PRIMARY KEY NOT NULL, mood mood NOT NULL);",
        ],
        DualConnection::Sqlite(_) => &["CREATE TABLE feelings (\
                id INTEGER PRIMARY KEY NOT NULL, \
                mood TEXT NOT NULL CHECK (mood IN ('Happy', 'Sad', 'meh')));"],
    };
    for stmt in ddl {
        conn.batch_execute(stmt).expect("create schema");
    }
}

#[diesel_dualdb::test(pg, sqlite)]
fn enum_round_trips(conn: &mut DualConnection) {
    create_schema(conn);

    for (id, mood) in [(1, Mood::Happy), (2, Mood::Sad), (3, Mood::Neutral)] {
        diesel::insert_into(feelings::table)
            .values((feelings::id.eq(id), feelings::mood.eq(mood)))
            .execute(conn)
            .expect("insert mood");
    }

    let got: Mood = feelings::table
        .select(feelings::mood)
        .filter(feelings::id.eq(3))
        .first(conn)
        .expect("select renamed variant");
    assert_eq!(got, Mood::Neutral);

    let all: Vec<Mood> = feelings::table
        .select(feelings::mood)
        .order(feelings::id)
        .load(conn)
        .expect("load all moods");
    assert_eq!(all, vec![Mood::Happy, Mood::Sad, Mood::Neutral]);
}
