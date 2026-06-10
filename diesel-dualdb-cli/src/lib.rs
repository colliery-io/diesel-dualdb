//! diesel-dualdb schema generator.
//!
//! Reads a directory of **logical** migrations (`<version>_<name>/up.sql` with
//! logical column types like `UUID`/`TIMESTAMP`/`JSON`) and produces:
//!   - a Postgres migration tree (native `uuid`/`timestamptz`/`jsonb`/`bytea`),
//!   - a SQLite migration tree (`BLOB`/`TEXT`),
//!   - a unified `schema.rs` (`table!` per table + `joinable!` /
//!     `allow_tables_to_appear_in_same_query!`, typed with the portable
//!     `diesel_dualdb::sql_types::*` markers).
//!
//! The generated migration trees are ordinary diesel migrations, so
//! `diesel migration run` applies them per backend.
//!
//! ## How it works
//!
//! Migration DDL is rendered by parsing each statement, **rewriting only the
//! column types** to each backend's native type, and re-emitting via
//! `sqlparser`'s `Display` — so constraints, foreign keys, defaults, indexes,
//! and `ALTER`s pass through unchanged. A separate pass builds the model that
//! drives `schema.rs`.
//!
//! ## Per-backend escape hatch
//!
//! Genuinely backend-specific DDL goes in a tagged block, emitted verbatim to
//! that backend only and ignored by the schema model:
//!
//! ```sql
//! -- dualdb:postgres
//! CREATE INDEX idx ON t USING gin (col);
//! -- dualdb:end
//! ```

use std::fs;
use std::path::Path;

use sqlparser::ast::{
    AlterTableOperation, ColumnOption, DataType, Ident, ObjectName, Statement, TableConstraint,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Postgres,
    Sqlite,
}

/// One logical type's three renderings.
#[derive(Clone, Copy)]
pub struct Mapped {
    pub pg: &'static str,
    pub sqlite: &'static str,
    /// Fully-qualified diesel SQL-type marker for `schema.rs`.
    pub marker: &'static str,
}

impl Mapped {
    fn native(&self, backend: Backend) -> &'static str {
        match backend {
            Backend::Postgres => self.pg,
            Backend::Sqlite => self.sqlite,
        }
    }
}

/// The mapping table: logical SQL type → (PG native, SQLite native, marker).
/// Native names are single tokens so they can be re-injected into the AST.
pub fn map_type(dt: &DataType) -> Option<Mapped> {
    let m = |pg, sqlite, marker| Some(Mapped { pg, sqlite, marker });
    match dt {
        DataType::Uuid => m("UUID", "BLOB", "diesel_dualdb::sql_types::Uuid"),
        DataType::Timestamp(_, _) => {
            m("TIMESTAMPTZ", "TEXT", "diesel_dualdb::sql_types::Timestamp")
        }
        DataType::JSON | DataType::JSONB => m("JSONB", "TEXT", "diesel_dualdb::sql_types::Json"),
        DataType::Bytea => m("BYTEA", "BLOB", "diesel_dualdb::sql_types::Bytes"),
        // Precision/scale is dropped (native name is a single token); the value
        // is stored exactly as `numeric` (PG) / TEXT (SQLite) regardless.
        DataType::Numeric(_) | DataType::Decimal(_) | DataType::Dec(_) => {
            m("NUMERIC", "TEXT", "diesel_dualdb::sql_types::Decimal")
        }
        DataType::Text => m("TEXT", "TEXT", "diesel::sql_types::Text"),
        DataType::SmallInt(_) => m("SMALLINT", "SMALLINT", "diesel::sql_types::SmallInt"),
        DataType::Integer(_) | DataType::Int(_) => {
            m("INTEGER", "INTEGER", "diesel::sql_types::Integer")
        }
        DataType::BigInt(_) => m("BIGINT", "BIGINT", "diesel::sql_types::BigInt"),
        DataType::Real | DataType::Float(_) => m("REAL", "REAL", "diesel::sql_types::Float"),
        DataType::Boolean | DataType::Bool => m("BOOLEAN", "INTEGER", "diesel::sql_types::Bool"),
        _ => None,
    }
}

fn native_data_type(dt: &DataType, backend: Backend, ctx: &str) -> Result<DataType> {
    let mapped = map_type(dt).ok_or_else(|| format!("{ctx}: no mapping for type {dt:?}"))?;
    Ok(DataType::Custom(
        ObjectName(vec![Ident::new(mapped.native(backend))]),
        vec![],
    ))
}

/// Last identifier of a (possibly schema-qualified) object name.
fn last_ident(name: &ObjectName) -> String {
    name.0
        .last()
        .map(|p| p.value.clone())
        .unwrap_or_else(|| name.to_string())
}

// ===== schema.rs model =====

struct ModelColumn {
    name: String,
    marker: &'static str,
    nullable: bool,
}

struct ModelTable {
    name: String,
    columns: Vec<ModelColumn>,
    primary_key: Vec<String>,
    /// (local column, parent table)
    foreign_keys: Vec<(String, String)>,
}

impl ModelTable {
    fn primary_key_cols(&self) -> Vec<String> {
        if self.primary_key.is_empty() {
            self.columns
                .first()
                .map(|c| vec![c.name.clone()])
                .unwrap_or_default()
        } else {
            self.primary_key.clone()
        }
    }
}

#[derive(Default)]
struct Model {
    tables: Vec<ModelTable>,
}

impl Model {
    fn table_mut(&mut self, name: &str) -> Option<&mut ModelTable> {
        self.tables.iter_mut().find(|t| t.name == name)
    }

    /// Fold a parsed statement into the model (the `schema.rs` source of truth).
    fn apply(&mut self, stmt: &Statement) -> Result<()> {
        match stmt {
            Statement::CreateTable(ct) => {
                let name = last_ident(&ct.name);
                let mut columns = Vec::new();
                let mut primary_key = Vec::new();
                let mut foreign_keys = Vec::new();

                for c in &ct.columns {
                    let mapped = map_type(&c.data_type).ok_or_else(|| {
                        format!(
                            "table `{name}`, column `{}`: no mapping for type {:?}",
                            c.name, c.data_type
                        )
                    })?;
                    let mut nullable = true;
                    for opt in &c.options {
                        match &opt.option {
                            ColumnOption::NotNull => nullable = false,
                            ColumnOption::Unique {
                                is_primary: true, ..
                            } => {
                                primary_key.push(c.name.value.clone());
                                nullable = false;
                            }
                            ColumnOption::ForeignKey { foreign_table, .. } => {
                                foreign_keys
                                    .push((c.name.value.clone(), last_ident(foreign_table)));
                            }
                            _ => {}
                        }
                    }
                    columns.push(ModelColumn {
                        name: c.name.value.clone(),
                        marker: mapped.marker,
                        nullable,
                    });
                }

                for con in &ct.constraints {
                    match con {
                        TableConstraint::PrimaryKey { columns, .. } => {
                            primary_key = columns.iter().map(|i| i.value.clone()).collect();
                        }
                        TableConstraint::ForeignKey {
                            columns,
                            foreign_table,
                            ..
                        } => {
                            for col in columns {
                                foreign_keys.push((col.value.clone(), last_ident(foreign_table)));
                            }
                        }
                        _ => {}
                    }
                }

                self.tables.push(ModelTable {
                    name,
                    columns,
                    primary_key,
                    foreign_keys,
                });
            }
            Statement::AlterTable {
                name, operations, ..
            } => {
                let tname = last_ident(name);
                for op in operations {
                    match op {
                        AlterTableOperation::AddColumn { column_def, .. } => {
                            let mapped = map_type(&column_def.data_type).ok_or_else(|| {
                                format!(
                                    "ALTER `{tname}` ADD `{}`: no mapping for type {:?}",
                                    column_def.name, column_def.data_type
                                )
                            })?;
                            let nullable = !column_def
                                .options
                                .iter()
                                .any(|o| matches!(o.option, ColumnOption::NotNull));
                            let col = ModelColumn {
                                name: column_def.name.value.clone(),
                                marker: mapped.marker,
                                nullable,
                            };
                            if let Some(t) = self.table_mut(&tname) {
                                t.columns.push(col);
                            } else {
                                return Err(format!("ALTER on unknown table `{tname}`"));
                            }
                        }
                        AlterTableOperation::DropColumn { column_name, .. } => {
                            if let Some(t) = self.table_mut(&tname) {
                                t.columns.retain(|c| c.name != column_name.value);
                            }
                        }
                        AlterTableOperation::AddConstraint(TableConstraint::ForeignKey {
                            columns,
                            foreign_table,
                            ..
                        }) => {
                            let parent = last_ident(foreign_table);
                            if let Some(t) = self.table_mut(&tname) {
                                for col in columns {
                                    t.foreign_keys.push((col.value.clone(), parent.clone()));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn render_schema_rs(&self) -> String {
        let mut out =
            String::from("// @generated by diesel-dualdb-schema — do not edit by hand.\n\n");

        let blocks: Vec<String> = self.tables.iter().map(render_table_macro).collect();
        out.push_str(&blocks.join("\n\n"));

        // joinable! per foreign key.
        let mut joinables = Vec::new();
        for t in &self.tables {
            for (col, parent) in &t.foreign_keys {
                joinables.push(format!(
                    "diesel::joinable!({} -> {} ({}));",
                    t.name, parent, col
                ));
            }
        }
        if !joinables.is_empty() {
            out.push_str("\n\n");
            out.push_str(&joinables.join("\n"));

            // allow_tables_to_appear_in_same_query! over all tables.
            let names: Vec<&str> = self.tables.iter().map(|t| t.name.as_str()).collect();
            out.push_str(&format!(
                "\n\ndiesel::allow_tables_to_appear_in_same_query!(\n    {},\n);",
                names.join(",\n    ")
            ));
        }

        out.push('\n');
        out
    }
}

fn render_use(path: &str, names: &[&str]) -> Option<String> {
    match names {
        [] => None,
        [one] => Some(format!("    use {path}::{one};\n")),
        many => Some(format!("    use {path}::{{{}}};\n", many.join(", "))),
    }
}

fn marker_parts(marker: &str) -> (&str, &str) {
    let idx = marker.rfind("::").expect("qualified marker path");
    (&marker[..idx], &marker[idx + 2..])
}

fn render_table_macro(t: &ModelTable) -> String {
    let mut diesel_names: Vec<&str> = Vec::new();
    let mut dualdb_names: Vec<&str> = Vec::new();
    for c in &t.columns {
        let (path, name) = marker_parts(c.marker);
        let bucket = if path.starts_with("diesel_dualdb") {
            &mut dualdb_names
        } else {
            &mut diesel_names
        };
        if !bucket.contains(&name) {
            bucket.push(name);
        }
    }
    // `Nullable<T>` wrapper for any nullable column comes from diesel's sql_types.
    if t.columns.iter().any(|c| c.nullable) && !diesel_names.contains(&"Nullable") {
        diesel_names.push("Nullable");
    }
    diesel_names.sort_unstable();
    dualdb_names.sort_unstable();

    let mut out = String::from("diesel::table! {\n");
    if let Some(line) = render_use("diesel::sql_types", &diesel_names) {
        out.push_str(&line);
    }
    if let Some(line) = render_use("diesel_dualdb::sql_types", &dualdb_names) {
        out.push_str(&line);
    }
    out.push_str(&format!(
        "\n    {} ({}) {{\n",
        t.name,
        t.primary_key_cols().join(", ")
    ));
    for c in &t.columns {
        let (_, name) = marker_parts(c.marker);
        let ty = if c.nullable {
            format!("Nullable<{name}>")
        } else {
            name.to_string()
        };
        out.push_str(&format!("        {} -> {},\n", c.name, ty));
    }
    out.push_str("    }\n}");
    out
}

// ===== migration DDL rendering (type-rewrite + Display) =====

fn parse(sql: &str) -> Result<Vec<Statement>> {
    Parser::parse_sql(&GenericDialect {}, sql).map_err(|e| format!("SQL parse error: {e}"))
}

/// Rewrite a statement's column types to `backend`'s native types in place.
fn rewrite_types(stmt: &mut Statement, backend: Backend) -> Result<()> {
    match stmt {
        Statement::CreateTable(ct) => {
            let tname = last_ident(&ct.name);
            for c in &mut ct.columns {
                let ctx = format!("table `{tname}` column `{}`", c.name);
                c.data_type = native_data_type(&c.data_type, backend, &ctx)?;
            }
        }
        Statement::AlterTable {
            name, operations, ..
        } => {
            let tname = last_ident(name);
            for op in operations.iter_mut() {
                if let AlterTableOperation::AddColumn { column_def, .. } = op {
                    let ctx = format!("ALTER `{tname}` ADD `{}`", column_def.name);
                    column_def.data_type = native_data_type(&column_def.data_type, backend, &ctx)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Render the `up.sql` for one backend from the migration's segments.
fn render_up(segments: &[Segment], backend: Backend) -> Result<String> {
    let mut pieces = Vec::new();
    for seg in segments {
        match seg {
            Segment::Shared(sql) => {
                for mut stmt in parse(sql)? {
                    rewrite_types(&mut stmt, backend)?;
                    pieces.push(format!("{stmt};"));
                }
            }
            Segment::Tagged { backend: b, raw } if *b == backend => {
                pieces.push(raw.trim().to_string());
            }
            Segment::Tagged { .. } => {}
        }
    }
    Ok(pieces.join("\n\n"))
}

/// `DROP TABLE` for each table created in this migration's shared segments, in
/// reverse order — used only when a migration supplies no `down.sql`.
fn derive_down(segments: &[Segment]) -> Result<String> {
    let mut created = Vec::new();
    for seg in segments {
        if let Segment::Shared(sql) = seg {
            for stmt in parse(sql)? {
                if let Statement::CreateTable(ct) = stmt {
                    created.push(last_ident(&ct.name));
                }
            }
        }
    }
    Ok(created
        .iter()
        .rev()
        .map(|t| format!("DROP TABLE IF EXISTS {t};"))
        .collect::<Vec<_>>()
        .join("\n"))
}

// ===== escape-hatch segmentation =====

enum Segment {
    Shared(String),
    Tagged { backend: Backend, raw: String },
}

/// Split logical DDL into shared chunks and per-backend tagged blocks.
fn segment(sql: &str) -> Result<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut shared = String::new();
    let mut tagged: Option<(Backend, String)> = None;

    for line in sql.lines() {
        let directive = line.trim().strip_prefix("-- dualdb:").map(str::trim);
        match directive {
            Some("postgres") | Some("sqlite") => {
                if tagged.is_some() {
                    return Err("nested `-- dualdb:` block".into());
                }
                if !shared.trim().is_empty() {
                    segments.push(Segment::Shared(std::mem::take(&mut shared)));
                } else {
                    shared.clear();
                }
                let backend = if directive == Some("postgres") {
                    Backend::Postgres
                } else {
                    Backend::Sqlite
                };
                tagged = Some((backend, String::new()));
            }
            Some("end") => {
                let (backend, raw) = tagged
                    .take()
                    .ok_or("`-- dualdb:end` without an open block")?;
                segments.push(Segment::Tagged { backend, raw });
            }
            Some(other) => return Err(format!("unknown directive `-- dualdb:{other}`")),
            None => {
                if let Some((_, raw)) = tagged.as_mut() {
                    raw.push_str(line);
                    raw.push('\n');
                } else {
                    shared.push_str(line);
                    shared.push('\n');
                }
            }
        }
    }
    if tagged.is_some() {
        return Err("unclosed `-- dualdb:` block (missing `-- dualdb:end`)".into());
    }
    if !shared.trim().is_empty() {
        segments.push(Segment::Shared(shared));
    }
    Ok(segments)
}

// ===== migrations directory I/O =====

pub struct Migration {
    pub dir_name: String,
    pub up_sql: String,
    pub down_sql: Option<String>,
}

pub fn read_migrations(dir: &Path) -> Result<Vec<Migration>> {
    let mut dirs: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("reading {}: {e}", dir.display()))?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let mut migrations = Vec::new();
    for path in dirs {
        let up = path.join("up.sql");
        if !up.exists() {
            continue;
        }
        let dir_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let up_sql =
            fs::read_to_string(&up).map_err(|e| format!("reading {}: {e}", up.display()))?;
        let down = path.join("down.sql");
        let down_sql = if down.exists() {
            Some(
                fs::read_to_string(&down)
                    .map_err(|e| format!("reading {}: {e}", down.display()))?,
            )
        } else {
            None
        };
        migrations.push(Migration {
            dir_name,
            up_sql,
            down_sql,
        });
    }
    Ok(migrations)
}

// ===== top-level generation =====

#[derive(Debug)]
pub struct RenderedMigration {
    pub dir_name: String,
    pub up_sql: String,
    pub down_sql: String,
}

#[derive(Debug)]
pub struct Generated {
    pub postgres: Vec<RenderedMigration>,
    pub sqlite: Vec<RenderedMigration>,
    pub schema_rs: String,
}

pub fn generate(migrations: &[Migration]) -> Result<Generated> {
    let mut model = Model::default();
    let mut postgres = Vec::new();
    let mut sqlite = Vec::new();

    for m in migrations {
        let ctx = |e: String| format!("migration `{}`: {e}", m.dir_name);
        let segments = segment(&m.up_sql).map_err(ctx)?;

        // Build the schema model from shared segments.
        for seg in &segments {
            if let Segment::Shared(sql) = seg {
                for stmt in parse(sql).map_err(ctx)? {
                    model.apply(&stmt).map_err(ctx)?;
                }
            }
        }

        let down = match &m.down_sql {
            Some(d) => d.trim_end().to_string(),
            None => derive_down(&segments).map_err(ctx)?,
        };
        postgres.push(RenderedMigration {
            dir_name: m.dir_name.clone(),
            up_sql: render_up(&segments, Backend::Postgres).map_err(ctx)?,
            down_sql: down.clone(),
        });
        sqlite.push(RenderedMigration {
            dir_name: m.dir_name.clone(),
            up_sql: render_up(&segments, Backend::Sqlite).map_err(ctx)?,
            down_sql: down,
        });
    }

    Ok(Generated {
        postgres,
        sqlite,
        schema_rs: model.render_schema_rs(),
    })
}

pub fn write_outputs(g: &Generated, out_dir: &Path) -> Result<()> {
    for (sub, set) in [
        ("migrations-postgres", &g.postgres),
        ("migrations-sqlite", &g.sqlite),
    ] {
        for rm in set {
            let dir = out_dir.join(sub).join(&rm.dir_name);
            fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
            write(&dir.join("up.sql"), &format!("{}\n", rm.up_sql))?;
            write(&dir.join("down.sql"), &format!("{}\n", rm.down_sql))?;
        }
    }
    write(&out_dir.join("schema.rs"), &g.schema_rs)
}

fn write(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).map_err(|e| format!("writing {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mig(name: &str, up: &str) -> Migration {
        Migration {
            dir_name: name.into(),
            up_sql: up.into(),
            down_sql: None,
        }
    }

    #[test]
    fn fk_composite_pk_index_and_escape_hatch() {
        let migs = vec![
            mig(
                "0001_users",
                "CREATE TABLE users (\
                    id UUID PRIMARY KEY NOT NULL, \
                    email TEXT NOT NULL);",
            ),
            mig(
                "0002_posts",
                "CREATE TABLE posts (\
                    id UUID NOT NULL, \
                    author UUID NOT NULL REFERENCES users(id), \
                    title TEXT NOT NULL, \
                    body JSON, \
                    PRIMARY KEY (id, author)); \
                 CREATE INDEX posts_author_idx ON posts (author);\n\
                 -- dualdb:postgres\n\
                 CREATE INDEX posts_body_gin ON posts USING gin (body);\n\
                 -- dualdb:end",
            ),
            mig("0003_alter", "ALTER TABLE users ADD COLUMN nickname TEXT;"),
        ];

        let g = generate(&migs).expect("generate");

        // Per-backend native types in DDL.
        let pg = g
            .postgres
            .iter()
            .map(|m| m.up_sql.clone())
            .collect::<String>();
        let sq = g
            .sqlite
            .iter()
            .map(|m| m.up_sql.clone())
            .collect::<String>();
        assert!(pg.contains("UUID"), "{pg}");
        assert!(pg.contains("JSONB"), "{pg}");
        assert!(sq.contains("BLOB"), "{sq}");
        assert!(
            sq.contains("body TEXT")
                || sq.contains("body TEXT,")
                || sq.to_uppercase().contains("BODY TEXT"),
            "{sq}"
        );

        // FK + index passed through to both; the GIN index only to Postgres.
        assert!(pg.contains("REFERENCES users"), "{pg}");
        assert!(pg.contains("posts_author_idx"), "{pg}");
        assert!(pg.contains("USING gin"), "{pg}");
        assert!(sq.contains("posts_author_idx"), "{sq}");
        assert!(
            !sq.contains("gin"),
            "sqlite must NOT get the gin index:\n{sq}"
        );

        // schema.rs: composite PK, the ALTER-added column, joinable!, allow_tables.
        let s = &g.schema_rs;
        assert!(s.contains("posts (id, author)"), "{s}");
        assert!(
            s.contains("nickname -> Nullable<Text>,"),
            "ALTER add missing:\n{s}"
        );
        assert!(s.contains("body -> Nullable<Json>,"), "{s}");
        assert!(
            s.contains("diesel::joinable!(posts -> users (author));"),
            "{s}"
        );
        assert!(s.contains("allow_tables_to_appear_in_same_query!"), "{s}");
    }

    #[test]
    fn unmapped_type_is_an_error() {
        let err = generate(&[mig("x", "CREATE TABLE t (id DATE NOT NULL);")]).unwrap_err();
        assert!(err.contains("no mapping for type"), "{err}");
        assert!(err.contains("migration `x`"), "{err}");
    }

    #[test]
    fn unclosed_escape_block_errors() {
        let err =
            generate(&[mig("x", "-- dualdb:postgres\nCREATE INDEX i ON t (c);")]).unwrap_err();
        assert!(err.contains("unclosed"), "{err}");
    }
}
