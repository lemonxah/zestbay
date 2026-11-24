#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_DIR"

PPA="" KEY="" REV=1 NOSIGN="" SERIES=()
while [ $# -gt 0 ]; do
    case "$1" in
        --upload) PPA="$2"; shift 2 ;;
        --key)    KEY="$2"; shift 2 ;;
        --rev)    REV="$2"; shift 2 ;;
        --no-sign) NOSIGN=1; shift ;;
        -*)       echo "unknown option $1" >&2; exit 1 ;;
        *)        SERIES+=("$1"); shift ;;
    esac
done
[ ${#SERIES[@]} -gt 0 ] || SERIES=(noble resolute)

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
WORK="$SCRIPT_DIR/ppa"
SRC="$WORK/zestbay-$VERSION"
ORIG="$WORK/zestbay_$VERSION.orig.tar.xz"

mkdir -p "$WORK"

if [ ! -f "$ORIG" ]; then
    echo "Creating $ORIG (with vendored crates)..."
    rm -rf "$SRC"
    mkdir -p "$SRC"
    tar -c \
        --exclude=./target --exclude=./.git --exclude=./builddir --exclude=./build-dir \
        --exclude=./.flatpak-builder --exclude=./repo --exclude=./dist --exclude='./.*' \
        --exclude=./debian --exclude=./pkg/deb/ppa --exclude='./pkg/deb/*.deb' \
        --exclude='./pkg/deb/zestbay_*' --exclude=./pkg/rpm/rpmbuild --exclude=./vendor \
        --exclude=./zestins/build --exclude='__pycache__' --exclude='*.py[cod]' \
        . | tar -x -C "$SRC"
    (cd "$SRC" && cargo vendor --locked vendor >/dev/null)
    python3 - "$SRC/vendor" <<'PY'
import json, pathlib, sys
for ck in pathlib.Path(sys.argv[1]).glob("*/.cargo-checksum.json"):
    d = json.loads(ck.read_text())
    for f in [f for f in d["files"] if f.endswith(".orig")]:
        del d["files"][f]
        (ck.parent / f).unlink(missing_ok=True)
    ck.write_text(json.dumps(d, separators=(",", ":")))
PY
    tar -C "$WORK" --sort=name --mtime='2026-01-01 00:00:00' --owner=0 --group=0 --numeric-owner \
        -cJf "$ORIG" "zestbay-$VERSION"
    rm -rf "$SRC"
fi

for series in "${SERIES[@]}"; do
    echo "=== $series ==="
    rm -rf "$SRC"
    mkdir -p "$WORK"
    tar -C "$WORK" -xJf "$ORIG"
    cp -r debian "$SRC/debian"
    cat > "$SRC/debian/changelog" <<EOL
zestbay ($VERSION-0ppa$REV~$series) $series; urgency=medium

  * Release $VERSION for $series.

 -- Ryno Kotze <lemon.xah@gmail.com>  $(date -R)
EOL
    SIGN=(); [ -n "$KEY" ] && SIGN=(-k"$KEY"); [ -n "$NOSIGN" ] && SIGN=(-us -uc)
    (cd "$SRC" && dpkg-buildpackage -S -sa -d -nc "${SIGN[@]}")
    rm -rf "$SRC"
    if [ -n "$PPA" ]; then
        dput "$PPA" "$WORK/zestbay_$VERSION-0ppa${REV}~${series}_source.changes"
    fi
done

echo "Done. Source packages are in $WORK"
