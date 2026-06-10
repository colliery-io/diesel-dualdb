"""Schema generation for diesel-dualdb.

    angreal schema gen     # regenerate schema/generated from schema/migrations
    angreal schema check   # fail if schema/generated is stale (CI gate)

The logical migrations live in `schema/migrations/`; the generated per-backend
migration trees + `schema.rs` are committed under `schema/generated/`.
"""

import shutil
import subprocess
import tempfile
from pathlib import Path

import angreal

PROJECT_ROOT = Path(angreal.get_root()).parent
SRC = "schema/migrations"
OUT = "schema/generated"

schema = angreal.command_group(name="schema", about="Schema / migration generation")


def _generate(out_dir: str) -> int:
    return subprocess.run(
        ["cargo", "run", "--quiet", "-p", "diesel-dualdb-cli", "--", SRC, out_dir],
        cwd=PROJECT_ROOT,
    ).returncode


@schema()
@angreal.command(name="gen", about="Regenerate schema/generated from schema/migrations")
def gen():
    rc = _generate(OUT)
    if rc == 0:
        print(f"regenerated {OUT}/ from {SRC}/")
    return rc


@schema()
@angreal.command(
    name="check",
    about="Fail if schema/generated is stale (run `angreal schema gen`)",
)
def check():
    tmp = tempfile.mkdtemp(prefix="dualdb-schema-")
    try:
        rc = _generate(tmp)
        if rc != 0:
            return rc
        diff = subprocess.run(["diff", "-r", tmp, str(PROJECT_ROOT / OUT)])
        if diff.returncode != 0:
            print("schema/generated is stale — run `angreal schema gen` and commit.")
            return 1
        print("schema/generated is up to date.")
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
