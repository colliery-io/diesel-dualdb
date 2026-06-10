-- Logical migration (diesel-dualdb): logical column types, one source for both
-- backends. Regenerate with `angreal schema gen`.
CREATE TABLE widgets (
    id UUID PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    data BYTEA NOT NULL,
    meta JSON,
    created_at TIMESTAMP NOT NULL
);
