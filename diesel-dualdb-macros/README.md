# diesel-dualdb-macros

Procedural macros for [`diesel-dualdb`](https://crates.io/crates/diesel-dualdb).

This crate is an implementation detail of `diesel-dualdb` and is not meant to be
depended on directly. It provides the proc-macros re-exported by the main crate:

- `#[diesel_dualdb::test(pg, sqlite)]` — run one test body against each backend.
- `bridge!(Marker, Newtype)` — generate a non-generic type's `MultiBackend`
  bridge in one line.
- `#[derive(DualEnum)]` — a portable enum (PostgreSQL native `enum` / SQLite
  `TEXT`).

Add the main crate instead:

```toml
[dependencies]
diesel-dualdb = "0.1"
```

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
