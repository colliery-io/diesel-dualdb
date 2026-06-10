//! Connectivity smoke test — validates the test orchestration wiring, not the
//! crate's own types (those come in DDB-T-0002+).
//!
//! SQLite always runs (in-memory). Postgres runs only when `DUALDB_PG_URL` is
//! set — the `angreal test all` task sets it after bringing the container up.
//! With no URL the Postgres test self-skips, which is exactly the behavior the
//! `#[dualdb::test]` harness will later formalize.

use diesel::prelude::*;
use diesel::sql_query;

#[test]
fn sqlite_in_memory_connects() {
    let mut conn = SqliteConnection::establish(":memory:").expect("connect to in-memory sqlite");
    sql_query("SELECT 1")
        .execute(&mut conn)
        .expect("run a trivial query on sqlite");
}

#[test]
fn postgres_connects_when_configured() {
    let Ok(url) = std::env::var("DUALDB_PG_URL") else {
        eprintln!("DUALDB_PG_URL not set — skipping Postgres connectivity test");
        return;
    };
    let mut conn = PgConnection::establish(&url).expect("connect to postgres from DUALDB_PG_URL");
    sql_query("SELECT 1")
        .execute(&mut conn)
        .expect("run a trivial query on postgres");
}
