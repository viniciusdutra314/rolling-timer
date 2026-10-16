#!/usr/bin/env python3
"""Collect full license notices from fetched Cargo sources, without new packages.

Usage: scripts/collect-licenses.py DESTINATION
Includes build/dev crates too: an overinclusive set is easier to audit than
silently guessing which conditional dependency ended up in a particular binary.
"""
import json
import pathlib
import shutil
import subprocess
import sys
import tarfile


def is_notice(path: pathlib.Path) -> bool:
    return any(path.name.upper().startswith(prefix) for prefix in (
        "LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "FTL.TXT",
    ))


def native_notices(destination: pathlib.Path, sources: list[str]) -> None:
    """Copy notices from pinned source trees/archives; never extract executables."""
    for source in sources:
        name, separator, filename = source.partition("=")
        if not separator or pathlib.Path(name).name != name:
            raise ValueError(f"Invalid native source specification: {source}")
        root = pathlib.Path(filename)
        output = destination / name
        if root.is_dir():
            for file in root.rglob("*"):
                if file.is_file() and is_notice(file):
                    target = output / file.relative_to(root)
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(file, target)
        else:
            with tarfile.open(root) as archive:
                for member in archive.getmembers():
                    path = pathlib.Path(member.name)
                    if not member.isfile() or not is_notice(path):
                        continue
                    if path.is_absolute() or ".." in path.parts:
                        raise ValueError(f"Unsafe notice path: {member.name}")
                    content = archive.extractfile(member)
                    if content is None:
                        raise ValueError(f"Cannot read notice: {member.name}")
                    target = output / path
                    target.parent.mkdir(parents=True, exist_ok=True)
                    with content, target.open("wb") as stream:
                        shutil.copyfileobj(content, stream)


def collect(destination: pathlib.Path) -> None:
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--offline", "--format-version", "1",
    ], text=True))
    destination.mkdir(parents=True, exist_ok=True)
    index = []
    for package in metadata["packages"]:
        if package["source"] is None:
            continue
        root = pathlib.Path(package["manifest_path"]).parent
        output = destination / f'{package["name"]}-{package["version"]}'
        notices = []
        for file in root.rglob("*"):
            # Include nested bundled native notices (FLTK, SQLite) as well.
            if file.is_file() and is_notice(file):
                relative = file.relative_to(root)
                target = output / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(file, target)
                notices.append(str(relative))
        index.append({
            "name": package["name"], "version": package["version"],
            "license": package["license"], "repository": package["repository"],
            "notices": sorted(notices),
        })
    (destination / "index.json").write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    try:
        if len(sys.argv) >= 4 and sys.argv[1] == "--native":
            native_notices(pathlib.Path(sys.argv[2]), sys.argv[3:])
        elif len(sys.argv) == 2:
            collect(pathlib.Path(sys.argv[1]))
        else:
            raise ValueError("Usage: collect-licenses.py DESTINATION | --native DESTINATION NAME=SOURCE ...")
    except (OSError, subprocess.CalledProcessError, ValueError, tarfile.TarError) as error:
        raise SystemExit(f"Cannot collect license notices: {error}") from error
