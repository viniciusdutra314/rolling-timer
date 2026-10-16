#!/usr/bin/env bash
# Package one already-built executable; never upload or publish from this script.
set -euo pipefail
if [[ $# -lt 3 || $# -gt 4 ]]; then
    echo 'Usage: package-release.sh BINARY TARGET OUTPUT_DIR [NATIVE_NOTICES]' >&2
    exit 2
fi
binary=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "$1")
target=$2
mkdir -p "$3" .scratch
output=$(cd "$3" && pwd)
stage=$(mktemp -d .scratch/package.XXXXXX)
trap 'rm -rf "$stage"' EXIT
suffix=''
if [[ $target == *windows* ]]; then suffix=.exe; fi
cp "$binary" "$stage/rolling-timer$suffix"
chmod +x "$stage/rolling-timer$suffix"
cp LICENSE docs/THIRD_PARTY.md "$stage/"
python3 scripts/collect-licenses.py "$stage/licenses/rust"
if [[ -n ${4:-} ]]; then
    cp -r "$4" "$stage/licenses/native"
    # Nix store directories are read-only. Make only our disposable copy
    # writable so both archive recipients and the EXIT cleanup can remove it.
    chmod -R u+w "$stage/licenses/native"
fi
name="rolling-timer-$target"
if [[ $target == *windows* ]]; then
    (cd "$stage" && python3 -m zipfile -c "$output/$name.zip" ./*)
    archive="$output/$name.zip"
else
    tar -czf "$output/$name.tar.gz" -C "$stage" .
    archive="$output/$name.tar.gz"
fi
python3 - "$archive" <<'PY'
import hashlib, pathlib, sys
archive = pathlib.Path(sys.argv[1])
with archive.open('rb') as stream:
    digest = hashlib.file_digest(stream, 'sha256').hexdigest()
archive.with_name(archive.name + '.sha256').write_text(digest + '  ' + archive.name + '\n', encoding='utf-8')
PY
printf '%s\n' "$archive"
