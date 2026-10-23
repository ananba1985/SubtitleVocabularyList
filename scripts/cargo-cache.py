"""Retire this application's previous-version Cargo caches, retaining dependencies.

Called by development/build entries and version changes. Cargo's own profile locks
are acquired nonblocking: a concurrent build defers the entire cleanup.
"""

import argparse
from contextlib import ExitStack, contextmanager
import json
import os
from pathlib import Path
import re
import shutil
import stat
import sys
import tomllib


STATE = ".svl-cache-version.json"


def safe_tree(path, target):
    """Only allow real paths inside this target, with no directory links."""
    path = path.absolute()
    path.relative_to(target)
    for part in (path, *path.parents):
        if part.exists() and part.lstat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT:
            raise ValueError(f"Directory link, cleanup deferred: {part}")
    data = os.environ.get("SVL_DATA_DIR")
    if data:
        data = Path(data).absolute()
        if path == data or path in data.parents or data in path.parents:
            raise ValueError(f"SVL_DATA_DIR overlaps cache, cleanup deferred: {path}")
    size = 0
    if path.is_dir():
        for parent, dirs, files in os.walk(path, followlinks=False):
            for name in dirs + files:
                child = Path(parent, name)
                info = child.lstat()
                if info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT:
                    raise ValueError(f"Directory link, cleanup deferred: {child}")
                if child.is_file():
                    size += info.st_size
    elif path.exists():
        size = path.stat().st_size
    return size


@contextmanager
def cargo_lock(profile):
    # Cargo locks <target>/<profile>/.cargo-lock, including cross-target profiles.
    # A Windows byte-range lock overlaps Cargo's LockFileEx whole-file lock.
    import msvcrt

    with (profile / ".cargo-lock").open("a+b") as handle:
        handle.seek(0)
        try:
            msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
        except OSError as error:
            raise BlockingIOError(f"Cargo build active: {profile}") from error
        try:
            yield
        finally:
            handle.seek(0)
            msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)


def profiles(target):
    roots = [target]
    if target.exists():
        roots += [p for p in target.iterdir() if p.is_dir() and re.fullmatch(r"[\w]+(?:-[\w.]+){2,}", p.name)]
    return [root / name for root in roots for name in ("debug", "release") if (root / name).is_dir()]


def project_caches(profile, package):
    package_name = package["package"]["name"]
    crates = {package_name.replace("-", "_"), package["lib"]["name"]}
    examples = {p["name"].replace("-", "_") for p in package.get("example", [])}
    crates.update(p["name"].replace("-", "_") for p in package.get("bin", []))
    # Only exact application crate/example names; third-party crate caches stay.
    incremental = re.compile(r"(?:" + "|".join(map(re.escape, crates | examples)) + r")-[a-z0-9]+")
    package_dir = re.compile(re.escape(package_name) + r"-[0-9a-f]{16}")
    artifact = re.compile(r"(?:lib)?(?:" + "|".join(map(re.escape, crates)) + r")(?:-[0-9a-f]{16})?\.(?:d|rlib|rmeta|lib|a|so|dll|dll\.lib|dll\.exp|pdb|exe|exp)")
    example = re.compile(r"(?:" + "|".join(map(re.escape, examples)) + r")(?:-[0-9a-f]{16})?\.(?:d|pdb|exe)") if examples else None
    for directory, pattern in (("incremental", incremental), (".fingerprint", package_dir), ("build", package_dir), ("deps", artifact), ("examples", example)):
        parent = profile / directory
        if parent.is_dir() and pattern:
            for path in parent.iterdir():
                if pattern.fullmatch(path.name):
                    yield path


def maintain(workspace, check_only=False):
    workspace = workspace.absolute()
    target = workspace / "src-tauri" / "target"
    package = tomllib.loads((workspace / "src-tauri" / "Cargo.toml").read_text(encoding="utf-8"))
    version = json.loads((workspace / "package.json").read_text(encoding="utf-8"))["version"]
    if package["package"]["name"] != "subtitle-vocabulary-list" or package["package"]["version"] != version:
        raise ValueError("Application versions differ; cache cleanup deferred.")
    if os.environ.get("CARGO_TARGET_DIR") or os.environ.get("CARGO_BUILD_BUILD_DIR"):
        print("Cargo cache cleanup deferred: custom target/build directory is configured.")
        return {"deferred": True, "removedBytes": 0}
    all_profiles = profiles(target)
    stale = []
    for profile in all_profiles:
        state = profile / STATE
        try:
            saved = json.loads(state.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            saved = {}
        if saved.get("version") != version:
            stale.append(profile)
    if not stale:
        return {"version": version, "removedBytes": 0, "profiles": []}

    # Preflight all paths before deleting anything. Top-level executables, Data,
    # tools, bundles and source/test fixtures are never cleanup candidates.
    removed = 0
    try:
        with ExitStack() as locks:
            for profile in all_profiles:
                safe_tree(profile / ".cargo-lock", target)
                locks.enter_context(cargo_lock(profile))
            candidates = [(p, safe_tree(p, target)) for profile in stale for p in project_caches(profile, package)]
            if check_only:
                for profile in stale:
                    print(f"Plan: retire application caches for {profile} (keep dependencies; current {version}).")
                print(f"Selected application cache files: {sum(size for _, size in candidates) / 1024**3:.2f} GiB (logical size).")
                return {"version": version, "removedBytes": 0, "plannedBytes": sum(size for _, size in candidates)}
            for path, size in candidates:
                if path.is_dir():
                    shutil.rmtree(path)
                else:
                    path.unlink()
                removed += size
            for profile in stale:
                state = profile / STATE
                temporary = profile / (STATE + ".tmp")
                safe_tree(state, target) if state.exists() else None
                safe_tree(temporary, target) if temporary.exists() else None
                temporary.write_text(json.dumps({"version": version}), encoding="utf-8")
                temporary.replace(state)
            print(f"Cargo caches now track {version}: retired {removed / 1024**3:.2f} GiB of application cache files; dependencies and Data retained.")
    except (BlockingIOError, PermissionError) as error:
        print(f"Cargo cache cleanup deferred; no process was stopped: {error}")
        return {"version": version, "deferred": True, "removedBytes": removed}
    return {"version": version, "removedBytes": removed, "profiles": [str(p) for p in stale]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-only", action="store_true")
    options = parser.parse_args()
    try:
        maintain(Path(__file__).resolve().parent.parent, options.check_only)
        return 0
    except (OSError, ValueError) as error:
        print(f"Cargo cache cleanup deferred: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
