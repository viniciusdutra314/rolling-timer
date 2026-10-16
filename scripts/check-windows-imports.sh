#!/usr/bin/env bash
# A successful MSVC link alone does not prove the CRT was embedded.
set -euo pipefail
binary=${1:?Windows executable path required}
llvm-readobj --coff-imports "$binary" | python3 -c '
import sys
names = [line.split("Name: ", 1)[1].strip() for line in sys.stdin if "Name: " in line]
if not names:
    raise SystemExit("No PE imports found; cannot verify the executable")
runtime = [name for name in names if name.lower().startswith(("vcruntime", "msvcp", "msvcr", "ucrtbase", "api-ms-win-crt-"))]
if runtime:
    raise SystemExit("Unexpected external CRT: " + ", ".join(runtime))
print("PE imports (no external CRT): " + ", ".join(names))
'
