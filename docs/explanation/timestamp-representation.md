# Explanation: timestamp representation

`types::Timestamp` wraps `DateTime<Utc>`. Postgres stores it natively as
`timestamptz`. SQLite has no native timestamp type, so it must be encoded — and
that encoding is effectively **permanent** (once rows exist, changing it is a
data migration) and determines whether range queries and `ORDER BY` behave.

## The requirement

SQLite compares the stored representation directly. So the hard requirement is:
**lexicographic (string) ordering of the stored value must equal chronological
ordering**, so `WHERE ts > $x` and `ORDER BY ts` just work, with no special
handling on the SQLite arm.

## The decision: fixed-format RFC3339 TEXT

SQLite stores timestamps as RFC 3339 / ISO 8601 **TEXT**, in a single fixed,
normalized format:

- **UTC only** — converted to UTC before formatting (the wrapper is
  `DateTime<Utc>`, so this is total).
- **`Z` suffix**, never a numeric offset — a fixed-width zone marker keeps all
  values mutually comparable.
- **Fixed fractional precision** — always six subsecond digits
  (`%Y-%m-%dT%H:%M:%S%.6fZ`). Variable precision breaks ordering
  (`…:01Z` would sort after `…:01.5Z`).
- Fixed-width fields throughout, so every stored string is the same length and
  string comparison equals time comparison.

## Why TEXT and not a unix integer

A unix-microseconds `INTEGER` is more compact (8 bytes vs ~27) and sorts
correctly by construction. TEXT was chosen anyway for **debuggability** — values
are legible in `sqlite3` shells, dumps, and logs, which matters for a library
others adopt and inspect. TEXT's one weakness (correct sorting depends on a fixed
format) is fully neutralized by freezing the format and enforcing it in a single
encoder. The integer option is recorded as the documented fallback if a workload
ever proves range-query or index performance demands it.

## Consequences

- Sortable, range-queryable timestamps on SQLite with no per-query special
  casing.
- Microsecond precision on both backends (Postgres `timestamptz` is also
  microsecond); finer precision is truncated.
- The fixed format is load-bearing: any code path that writes a differently
  formatted string would silently corrupt ordering — which is why there's a
  single canonical encoder and round-trip + ordering tests.
