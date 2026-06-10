# diesel-dualdb-cli

Schema & migration generator for
[`diesel-dualdb`](https://crates.io/crates/diesel-dualdb): one logical-DDL
migration source becomes per-backend migration trees plus a unified `schema.rs`.

## Install

```sh
cargo install diesel-dualdb-cli
```

This installs the `diesel-dualdb-schema` binary.

## Usage

```sh
diesel-dualdb-schema <migrations_dir> <out_dir>
```

Reads logical migrations from `<migrations_dir>` and writes
`migrations-postgres/`, `migrations-sqlite/`, and `schema.rs` under `<out_dir>`.
Foreign keys become `joinable!`, with support for composite PKs, indexes,
`ALTER`, and a `-- dualdb:<backend>` per-backend escape hatch.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
