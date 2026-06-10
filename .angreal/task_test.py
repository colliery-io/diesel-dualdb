"""Test orchestration for diesel-dualdb.

SQLite tests run in-memory and need nothing external. Postgres tests run
against a disposable container defined in ``docker-compose.yml`` — these tasks
bring it up, wait for it to accept connections, point the suite at it via
``DUALDB_PG_URL``, and tear it down afterward.

Commands:
    angreal test all       # SQLite + Postgres (spins the container up/down)
    angreal test sqlite    # SQLite only, no container (Pg tests self-skip)
    angreal db up          # Start the Postgres container and leave it running
    angreal db down        # Stop and remove it
"""

import os
import subprocess
import time
from pathlib import Path

import angreal
from angreal.integrations.docker import DockerCompose

PROJECT_ROOT = Path(angreal.get_root()).parent
COMPOSE_FILE = str(PROJECT_ROOT / "docker-compose.yml")
PROJECT_NAME = "dualdb-test"

# Matches docker-compose.yml (user/password/db on the published 55432 port).
PG_URL = "postgres://dualdb:dualdb@localhost:55432/dualdb_test"


def _compose():
    return DockerCompose(COMPOSE_FILE, project_name=PROJECT_NAME)


def _wait_for_pg(dc, timeout=60):
    """Poll pg_isready *inside* the container until it accepts connections."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        result = dc.exec(
            "postgres",
            ["pg_isready", "-U", "dualdb", "-d", "dualdb_test"],
            tty=False,
        )
        if result.success:
            return True
        time.sleep(1)
    return False


def _cargo_test(env):
    # --workspace so the macro + cli crates' tests run too.
    return subprocess.run(
        ["cargo", "test", "--workspace"], cwd=PROJECT_ROOT, env=env
    ).returncode


test = angreal.command_group(name="test", about="Run the test suite")
db = angreal.command_group(name="db", about="Manage the test Postgres container")


@test()
@angreal.command(
    name="all",
    about="Run tests against SQLite (in-memory) and a Postgres container",
)
@angreal.argument(
    name="keep",
    long="keep",
    is_flag=True,
    takes_value=False,
    help="Leave the Postgres container running after the tests finish",
)
def test_all(keep=False):
    if not DockerCompose.is_available():
        print("Docker Compose is not available — is the Docker daemon running?")
        return 1

    dc = _compose()
    print("Starting Postgres container...")
    up = dc.up(detach=True)
    if not up.success:
        print(up.stderr)
        return 1

    try:
        if not _wait_for_pg(dc):
            print("Postgres did not become ready within the timeout.")
            return 1
        print(f"Postgres ready at {PG_URL}")
        env = dict(os.environ, DUALDB_PG_URL=PG_URL)
        return _cargo_test(env)
    finally:
        if keep:
            print(f"Leaving Postgres running ({PG_URL}); stop with `angreal db down`.")
        else:
            dc.down(volumes=True)


@test()
@angreal.command(
    name="sqlite",
    about="Run only the SQLite-backed tests (no container; Pg tests self-skip)",
)
def test_sqlite():
    env = dict(os.environ)
    env.pop("DUALDB_PG_URL", None)  # ensure the Pg tests skip themselves
    return _cargo_test(env)


@db()
@angreal.command(name="up", about="Start the Postgres test container and wait for it")
def db_up():
    if not DockerCompose.is_available():
        print("Docker Compose is not available — is the Docker daemon running?")
        return 1
    dc = _compose()
    up = dc.up(detach=True)
    if up.success and _wait_for_pg(dc):
        print(f"Postgres ready at {PG_URL}")
        print("Export it with: export DUALDB_PG_URL='%s'" % PG_URL)
        return 0
    print(up.stderr)
    return 1


@db()
@angreal.command(name="down", about="Stop and remove the Postgres test container")
def db_down():
    dc = _compose()
    return 0 if dc.down(volumes=True).success else 1
