#!/usr/bin/env bash
set -euo pipefail
binary=${1:?binary path required}
ls -lh "$binary"
case $(uname -s) in
    Linux) file "$binary"; readelf -d "$binary" | sed -n '/NEEDED/p' ;;
    Darwin) file "$binary"; otool -L "$binary" ;;
    *) echo 'Use dumpbin /DEPENDENTS on Windows' ;;
esac
