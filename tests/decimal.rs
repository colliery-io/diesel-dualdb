//! `Decimal` round-trips on both backends (PG native `numeric`, SQLite TEXT).
#![cfg(feature = "decimal")]

use std::str::FromStr;

use bigdecimal::BigDecimal;
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_dualdb::types::Decimal;
use diesel_dualdb::DualConnection;

diesel::table! {
    use diesel::sql_types::Integer;
    use diesel_dualdb::sql_types::Decimal;

    money (id) {
        id -> Integer,
        amount -> Decimal,
    }
}

fn create_table(conn: &mut DualConnection) {
    let ddl = match &*conn {
        DualConnection::Pg(_) => {
            "CREATE TEMP TABLE money (id INTEGER PRIMARY KEY NOT NULL, amount NUMERIC NOT NULL);"
        }
        DualConnection::Sqlite(_) => {
            "CREATE TABLE money (id INTEGER PRIMARY KEY NOT NULL, amount TEXT NOT NULL);"
        }
    };
    conn.batch_execute(ddl).expect("create table");
}

#[diesel_dualdb::test(pg, sqlite)]
fn decimal_round_trips(conn: &mut DualConnection) {
    create_table(conn);

    let amount = Decimal(BigDecimal::from_str("12345.6789").unwrap());
    diesel::insert_into(money::table)
        .values((money::id.eq(1), money::amount.eq(amount.clone())))
        .execute(conn)
        .expect("insert decimal");

    let got: Decimal = money::table
        .select(money::amount)
        .filter(money::id.eq(1))
        .first(conn)
        .expect("select decimal");
    assert_eq!(got, amount);

    // A negative, high-precision value too.
    let neg = Decimal(BigDecimal::from_str("-0.000000000001").unwrap());
    diesel::insert_into(money::table)
        .values((money::id.eq(2), money::amount.eq(neg.clone())))
        .execute(conn)
        .expect("insert negative decimal");
    let got: Decimal = money::table
        .select(money::amount)
        .filter(money::id.eq(2))
        .first(conn)
        .expect("select negative decimal");
    assert_eq!(got, neg);
}
