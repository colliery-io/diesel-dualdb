# diesel-dualdb documentation

Write Diesel query code **once** and run it on both PostgreSQL and SQLite.

These docs follow [Diátaxis](https://diataxis.fr/): four kinds of documentation,
each serving a different need. Start wherever your need lives.

| If you want to… | Go to | Mode |
|---|---|---|
| **learn** by building something | [Tutorials](tutorials/) | learning-oriented |
| **do** a specific task | [How-to guides](how-to/) | task-oriented |
| **look up** an exact detail | [Reference](reference/) | information-oriented |
| **understand** why it works this way | [Explanation](explanation/) | understanding-oriented |

## Map

### Tutorials — start here if you're new
- [Getting started](tutorials/getting-started.md) — from an empty crate to a row
  round-tripping on both backends.

### How-to guides
- [Test on both backends](how-to/test-on-both-backends.md)
- [Generate schema & migrations](how-to/generate-schema-and-migrations.md)
- [Add a portable type](how-to/add-a-portable-type.md)
- [Diverge per backend (the escape hatch)](how-to/diverge-per-backend.md)
- [Pool connections](how-to/pool-connections.md)

### Reference
- [Portable types](reference/portable-types.md)
- [Macros](reference/macros.md) — `#[diesel_dualdb::test]`, `bridge!`
- [Schema generator](reference/schema-gen.md)
- [Cargo features & MSRV](reference/features-and-msrv.md)

### Explanation
- [Design philosophy](explanation/design-philosophy.md) — one arm by default
- [Architecture](explanation/architecture.md) — `MultiBackend` and the bridge
- [Schema generation](explanation/schema-generation.md) — why a logical schema
- [Timestamp representation](explanation/timestamp-representation.md)

## Status

Sync backend, v1. The portable type layer, the `MultiBackend` bridge, the
test/bridge macros, the schema generator, connection pooling with URL detection
(`Pool::connect`), and the `dispatch` escape hatch all work today. **Async** is
the planned fast-follow — see [Design philosophy](explanation/design-philosophy.md)
for the roadmap. Anything not documented here as working should be assumed not
yet built.
