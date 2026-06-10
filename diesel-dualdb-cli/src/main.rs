//! CLI for the diesel-dualdb schema generator.
//!
//!     diesel-dualdb-schema <migrations_dir> <out_dir>
//!
//! Reads logical migrations from `<migrations_dir>` and writes
//! `migrations-postgres/`, `migrations-sqlite/`, and `schema.rs` under
//! `<out_dir>`.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(migrations_dir), Some(out_dir)) = (args.next(), args.next()) else {
        eprintln!("usage: diesel-dualdb-schema <migrations_dir> <out_dir>");
        return ExitCode::from(2);
    };

    match run(&PathBuf::from(migrations_dir), &PathBuf::from(out_dir)) {
        Ok(count) => {
            println!("generated {count} migration(s) + schema.rs");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("diesel-dualdb-schema: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(
    migrations_dir: &std::path::Path,
    out_dir: &std::path::Path,
) -> diesel_dualdb_cli::Result<usize> {
    let migrations = diesel_dualdb_cli::read_migrations(migrations_dir)?;
    let generated = diesel_dualdb_cli::generate(&migrations)?;
    diesel_dualdb_cli::write_outputs(&generated, out_dir)?;
    Ok(migrations.len())
}
