"""Emulate a daemon with no access to client filesystem bind paths."""

import hashlib
import io
from pathlib import Path, PurePosixPath
import sys
import tarfile


args = sys.argv[1:]
fault = CONFIG["fault"]
state_path = Path(CONFIG["state"])


def record(operation, **fields):
    with Path(CONFIG["log"]).open("a", encoding="utf-8") as output:
        output.write(json.dumps({"operation": operation, "argv": args, **fields}) + "\n")


def fail(message, code=1):
    print(message, file=sys.stderr)
    raise SystemExit(code)


def option(name):
    return args[args.index(name) + 1]


def mounts():
    return [args[index + 1] for index, part in enumerate(args) if part == "--mount"]


def mount_fields(value):
    return dict(part.split("=", 1) if "=" in part else (part, True) for part in value.split(","))


def state():
    return json.loads(state_path.read_text())


def archive_files(payload, destination):
    assert payload, "snapshot upload must provide a bounded tar archive on stdin"
    assert len(payload) <= 3 * 1024 * 1024
    names = []
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:") as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            assert not path.is_absolute() and ".." not in path.parts
            assert member.isdir() or member.isfile()
            assert member.uid == 65534 and member.gid == 65534
            assert member.mtime == 0
            assert member.mode == (0o755 if member.isdir() else 0o444)
            if member.isfile():
                content = archive.extractfile(member).read()
                assert len(content) == member.size
                names.append(member.name)
                if destination == "/fixtures":
                    assert member.name == "engarg-preflight.txt"
                    assert content == b"engarg-read-only-fixture/1\n"
                else:
                    assert member.name == "main.rs"
                    assert b"/fixtures/engarg-preflight.txt" in content
    return sorted(names)


if any("type=bind" in argument for argument in args):
    record("bind rejected")
    fail("bind source path does not exist on this daemon", 125)

if args[0] == "image":
    record("image")
    if fault == "image":
        fail("synthetic image admission denial")
    print("sha256:" + "0" * 64)
elif args[0] == "create" or args[:2] == ["container", "create"]:
    name = option("--name")
    record("create", name=name)
    if fault == "create":
        fail("synthetic staging creation denial", 125)
    selected = [mount_fields(value) for value in mounts()]
    assert len(selected) == 2
    assert {item["target"] for item in selected} == {"/src", "/fixtures"}
    assert all(item["type"] == "volume" and item.get("volume-nocopy") for item in selected)
    assert all("source" not in item and "src" not in item for item in selected)
    assert "--env" not in args
    assert option("--pull") == "never"
    assert option("--network") == "none"
    owner = name.removesuffix("-inputs")
    assert option("--label") == "engarg.snapshot-owner=" + owner
    volume_names = {target: hashlib.sha256((name + target).encode()).hexdigest() for target in ["/src", "/fixtures"]}
    state_path.write_text(json.dumps({"stager": name, "owner": "foreign" if fault == "create-collision" else owner, "volumes": volume_names}))
    if fault in ["create-ambiguous", "create-collision"]:
        fail("synthetic staging creation denial", 125)
    print("1" * 64)
elif args[:2] == ["container", "inspect"] or args[0] == "inspect":
    ownership = ".Config.Labels" in option("--format")
    record("owner inspect" if ownership else "inspect")
    if not state_path.exists():
        fail("Error: No such container: " + args[-1])
    saved = state()
    assert args[-1] == saved["stager"]
    if ownership:
        print(saved["owner"])
        raise SystemExit(0)
    if fault == "inspect":
        fail("synthetic staging inspection denial")
    selected = [{"Type": "volume", "Name": name, "Destination": target, "RW": True} for target, name in saved["volumes"].items()]
    if fault == "inspect-malformed":
        selected[0]["Type"] = "bind"
    print(json.dumps(selected))
elif args[0] == "cp" or args[:2] == ["container", "cp"]:
    saved = state()
    destination = args[-1].split(":", 1)
    assert destination[0] == saved["stager"]
    assert destination[1] in ["/src", "/fixtures"]
    assert args[-2] == "-"
    files = archive_files(sys.stdin.buffer.read(), destination[1])
    record("cp", destination=destination[1], files=files)
    if fault == ("cp-source" if destination[1] == "/src" else "cp-fixtures"):
        fail("synthetic snapshot upload denial")
elif args[0] == "run":
    saved = state()
    record("run", name=option("--name"))
    selected = [mount_fields(value) for value in mounts()]
    assert len(selected) == 2
    assert {item["target"] for item in selected} == {"/src", "/fixtures"}
    assert all(item["type"] == "volume" and item.get("readonly") and item.get("volume-nocopy") for item in selected)
    assert all(item["source"] == saved["volumes"][item["target"]] for item in selected)
    assert "--read-only" in args
    assert option("--user") == "65534:65534"
    assert option("--network") == "none"
    assert option("--cap-drop") == "ALL"
    assert option("--security-opt") == "no-new-privileges"
    assert option("--pids-limit") == "64"
    assert option("--cpus") == "1"
    assert "--env" not in args
    assert sys.stdin.buffer.read() == b"engarg-toolchain-check/1\n"
    if fault == "run":
        fail("synthetic worker launch denial", 125)
    print('{"schema":"engarg.toolchain-probe/1","passed":true}')
elif args[:2] == ["container", "rm"] or args[0] == "rm":
    name = args[-1]
    record("container rm", name=name, volumes="--volumes" in args)
    if fault == "cleanup-worker" and not name.endswith("-inputs"):
        fail("synthetic worker cleanup denial")
elif args[0] == "start" or args[:2] == ["container", "start"]:
    record("start")
    fail("staging containers must remain stopped")
else:
    record("unexpected")
    fail("unexpected Docker command")
