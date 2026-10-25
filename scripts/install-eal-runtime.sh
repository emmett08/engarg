#!/usr/bin/env bash
# Install the EAL runtime pinned by this EngArg checkout into an isolated prefix.
# Usage: bash scripts/install-eal-runtime.sh [--root DIR]
# Success writes only the absolute eal-host path to stdout; build logs use stderr.
set -euo pipefail
exec python3 - "${BASH_SOURCE[0]}" "$@" <<'PY'
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from urllib.parse import parse_qs, urlsplit, urlunsplit


TOOLCHAIN = "1.90.0"
PACKAGES = ("eal-process", "eal-runtime", "eal-mcp", "eal-collector")
BINARIES = ("eal-host", "eal-mcp", "eal-method-worker",
            "eal-command-supervisor", "eal-process-supervisor", "eal-collector")
OWNER = ".engarg-eal-runtime-owner.json"
RECEIPT = ".engarg-eal-runtime.json"


def fail(message):
    raise ValueError(message)


def read_json(path):
    flags = os.O_RDONLY | os.O_NONBLOCK | getattr(os, "O_NOFOLLOW", 0)
    with os.fdopen(os.open(path, flags), "rb") as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
            fail("Runtime installation records must be regular files")
        raw = source.read(65_537)
    if len(raw) > 65_536:
        fail("Runtime installation record exceeds its size limit")
    return json.loads(raw)


def write_json(path, value):
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8",
                                     dir=path.parent, prefix=".runtime-", delete=False) as target:
        temporary = Path(target.name)
        try:
            json.dump(value, target, sort_keys=True, indent=2)
            target.write("\n")
            target.flush()
            os.fsync(target.fileno())
        except BaseException:
            temporary.unlink()
            raise
    try:
        os.replace(temporary, path)
    finally:
        if temporary.exists():
            temporary.unlink()


def pinned_source(repo):
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1",
         "--manifest-path", str(repo / "Cargo.toml")],
        cwd=repo, check=True, stdout=subprocess.PIPE, text=True,
    )
    metadata = json.loads(result.stdout)
    packages = [package for package in metadata["packages"] if package["name"] == "engarg"]
    if len(packages) != 1:
        fail("Cargo metadata must identify one EngArg package")
    dependencies = [dependency for dependency in packages[0]["dependencies"]
                    if dependency["name"].startswith("eal-")]
    if not {"eal-collector-contracts", "eal-process"}.issubset(
            {dependency["name"] for dependency in dependencies}):
        fail("Cargo metadata is missing EngArg's pinned EAL dependencies")
    pins = set()
    for dependency in dependencies:
        source = dependency.get("source") or ""
        if not source.startswith("git+"):
            fail("Every EAL dependency must use the same pinned Git source")
        parsed = urlsplit(source[4:])
        query = parse_qs(parsed.query, strict_parsing=True)
        revisions = query.get("rev", [])
        if (set(query) != {"rev"} or len(revisions) != 1
                or not re.fullmatch(r"[0-9a-f]{40}", revisions[0])
                or parsed.fragment not in ("", revisions[0])
                or parsed.scheme != "https" or not parsed.hostname
                or parsed.username or parsed.password or not parsed.path):
            fail("EAL dependencies require an HTTPS Git URL and an exact 40-character revision")
        pins.add((urlunsplit((parsed.scheme, parsed.netloc, parsed.path, "", "")), revisions[0]))
    if len(pins) != 1:
        fail("EngArg's EAL dependencies use different Git URLs or revisions")
    return next(iter(pins))


def binary_hash(path):
    flags = os.O_RDONLY | os.O_NONBLOCK | getattr(os, "O_NOFOLLOW", 0)
    with os.fdopen(os.open(path, flags), "rb") as source:
        before = os.fstat(source.fileno())
        if (not stat.S_ISREG(before.st_mode) or not before.st_mode & 0o111
                or not 0 < before.st_size <= 1_073_741_824):
            fail("Runtime companion must be a bounded regular executable: " + path.name)
        digest = hashlib.sha256()
        total = 0
        while True:
            block = source.read(65_536)
            if not block:
                break
            total += len(block)
            if total > 1_073_741_824:
                fail("Runtime companion exceeds its byte limit: " + path.name)
            digest.update(block)
        after = os.fstat(source.fileno())
    current = path.lstat()
    identity = lambda value: (value.st_dev, value.st_ino, value.st_size,
                              value.st_mtime_ns, value.st_ctime_ns)
    if (identity(before) != identity(after) or identity(before) != identity(current)
            or total != before.st_size or not stat.S_ISREG(current.st_mode)):
        fail("Runtime companion changed during inspection: " + path.name)
    return digest.hexdigest()


def hashes(prefix):
    directory = prefix / "bin"
    if directory.is_symlink() or not directory.is_dir():
        fail("Runtime prefix requires a regular bin directory")
    return {name: binary_hash(directory / name) for name in BINARIES}


def cache_valid(prefix, owner):
    try:
        receipt = read_json(prefix / RECEIPT)
        return (receipt.get("schema") == "engarg.eal-runtime/1"
                and receipt.get("git") == owner["git"]
                and receipt.get("revision") == owner["revision"]
                and receipt.get("binaries") == hashes(prefix))
    except (OSError, ValueError, TypeError, AttributeError):
        return False


def main():
    parser = argparse.ArgumentParser(description="Install EngArg's pinned Rust EAL runtime in an isolated prefix")
    parser.add_argument("--root", type=Path, help="empty or previously managed installation prefix")
    args = parser.parse_args(sys.argv[2:])
    repo = Path(sys.argv[1]).resolve().parent.parent
    if shutil.which("cargo") is None:
        fail("Required command: cargo")
    git_url, revision = pinned_source(repo)
    prefix = (args.root or repo / ".engarg-local" / "eal-runtime" / revision).expanduser().resolve()
    owner = {"schema": "engarg.eal-runtime-owner/1", "git": git_url, "revision": revision}
    if prefix.exists():
        if not prefix.is_dir():
            fail("Runtime installation prefix must be a directory")
        if any(prefix.iterdir()):
            try:
                existing_owner = read_json(prefix / OWNER)
            except (OSError, ValueError):
                fail("Refusing to overwrite a nonempty unmanaged runtime prefix: " + str(prefix))
            if existing_owner != owner:
                fail("Runtime prefix belongs to a different source revision: " + str(prefix))
    else:
        prefix.mkdir(mode=0o700, parents=True)
    os.chmod(prefix, 0o700)
    if not (prefix / OWNER).exists():
        write_json(prefix / OWNER, owner)
    flags = os.O_RDWR | os.O_CREAT | os.O_NONBLOCK | getattr(os, "O_NOFOLLOW", 0)
    with os.fdopen(os.open(prefix / ".engarg-install.lock", flags, 0o600), "a+b") as lock:
        if not stat.S_ISREG(os.fstat(lock.fileno()).st_mode):
            fail("Runtime installation lock must be a regular file")
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            fail("Another EAL installation is active in " + str(prefix))
        # Children inherit the lock so an interrupted helper cannot race a
        # still-running build. The kernel releases it after the last holder exits.
        # Keep its inode so callers cannot lock different files concurrently.
        if cache_valid(prefix, owner):
            print(prefix / "bin" / "eal-host")
            return
        if shutil.which("rustup") is None:
            fail("Required command for runtime installation: rustup")
        # A failure leaves ownership but never a success receipt. A later attempt
        # reinstalls every package, including the preferred legacy supervisor alias.
        receipt = prefix / RECEIPT
        if receipt.exists() or receipt.is_symlink():
            receipt.unlink()
        for name in ("bin", "target"):
            directory = prefix / name
            if directory.is_symlink() or (directory.exists() and not directory.is_dir()):
                fail("Runtime " + name + " must be a regular directory")
        # Remove known companion leaves first. A replaced symlink or hard link
        # must never redirect a forced reinstall into a global installation.
        for name in BINARIES:
            binary = prefix / "bin" / name
            if binary.is_symlink() or binary.exists():
                if binary.is_dir() and not binary.is_symlink():
                    fail("Runtime companion path is a directory: " + name)
                binary.unlink()
        subprocess.run(["rustup", "toolchain", "install", TOOLCHAIN, "--profile", "minimal"],
                       cwd=repo, check=True, stdout=sys.stderr, pass_fds=(lock.fileno(),))
        environment = dict(os.environ, CARGO_TARGET_DIR=str(prefix / "target"))
        for package in PACKAGES:
            subprocess.run(
                ["cargo", "+" + TOOLCHAIN, "install", "--git", git_url, "--rev", revision,
                 "--locked", "--root", str(prefix), "--force", package],
                cwd=repo, env=environment, check=True, stdout=sys.stderr,
                pass_fds=(lock.fileno(),),
            )
        write_json(receipt, {"schema": "engarg.eal-runtime/1", "git": git_url,
                             "revision": revision, "binaries": hashes(prefix)})
    print(prefix / "bin" / "eal-host")


try:
    main()
except subprocess.CalledProcessError as error:
    print("EAL runtime installation command failed with status " + str(error.returncode), file=sys.stderr)
    raise SystemExit(error.returncode or 1)
except (OSError, ValueError, KeyError, TypeError) as error:
    print("EAL runtime installation failed: " + str(error), file=sys.stderr)
    raise SystemExit(2)
PY
