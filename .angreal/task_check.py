"""Static checks for diesel-dualdb: feature matrix, formatting, lints.

    angreal check features   # cargo check across the feature matrix
    angreal check fmt        # cargo fmt --check
    angreal check clippy     # cargo clippy -D warnings (all features)
    angreal check all        # all of the above
"""

import subprocess
from pathlib import Path

import angreal

PROJECT_ROOT = Path(angreal.get_root()).parent

# The feature combinations that must compile. Each type lives behind its own
# feature, so every one must build alone and `--no-default-features` must too.
FEATURE_SETS = [
    ["--no-default-features"],
    [],  # default = all types
    ["--no-default-features", "--features", "uuid"],
    ["--no-default-features", "--features", "chrono"],
    ["--no-default-features", "--features", "serde_json"],
    ["--no-default-features", "--features", "decimal"],
    ["--no-default-features", "--features", "array"],
    ["--all-features"],
]

check = angreal.command_group(name="check", about="Static checks")


def _run(cmd):
    print("+ " + " ".join(cmd))
    return subprocess.run(cmd, cwd=PROJECT_ROOT).returncode


def _check_features():
    rc = 0
    for fs in FEATURE_SETS:
        rc |= _run(["cargo", "check", "--quiet", *fs])
    return rc


def _check_fmt():
    return _run(["cargo", "fmt", "--check"])


def _check_clippy():
    return _run(
        [
            "cargo",
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ]
    )


@check()
@angreal.command(name="features", about="cargo check across the feature matrix")
def features():
    return _check_features()


@check()
@angreal.command(name="fmt", about="cargo fmt --check")
def fmt():
    return _check_fmt()


@check()
@angreal.command(name="clippy", about="cargo clippy -D warnings (all features)")
def clippy():
    return _check_clippy()


@check()
@angreal.command(name="all", about="feature matrix + fmt + clippy")
def all_checks():
    return _check_features() | _check_fmt() | _check_clippy()
