#!/bin/bash
set -euo pipefail

# Build signed Debian *source* packages for a Launchpad PPA, one per Ubuntu series.
# Launchpad then compiles each against that release's own libraries, so unlike
# the single prebuilt .deb the dependencies always resolve (24.04, 26.04, ...).
#
# Usage:
#   pkg/deb/build-ppa.sh [--upload ppa:USER/NAME] [--key KEYID] [--rev N] [--no-sign] [SERIES...]
#
# SERIES defaults to: noble resolute   (24.04 and 26.04)
# Without --upload the .changes files are only built (in pkg/deb/ppa/).
#
# Prerequisites: dpkg-buildpackage (package dpkg-dev on Debian/Ubuntu, dpkg on
# Arch), cargo (for `cargo vendor`), python3, xz, and a GPG key registered on
# your Launchpad account. --upload also needs dput (or run it from a container).
# debhelper/devscripts are NOT needed here: Launchpad runs the actual build.

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

# The orig tarball must be byte-identical for every series, so build it once.
if [ ! -f "$ORIG" ]; then
    echo "Creating $ORIG (with vendored crates)..."
    rm -rf "$SRC"
    mkdir -p "$SRC"
    tar -c \
        --exclude=./target --exclude=./.git --exclude=./builddir --exclude=./build-dir \
        --exclude=./.flatpak-builder --exclude=./repo --exclude=./dist --exclude=./.claude \
        --exclude=./debian --exclude=./pkg/deb/ppa --exclude='./pkg/deb/*.deb' \
        --exclude='./pkg/deb/zestbay_*' --exclude=./pkg/rpm/rpmbuild --exclude=./vendor \
        --exclude=./zestins/build \
        . | tar -x -C "$SRC"
    (cd "$SRC" && cargo vendor --locked vendor >/dev/null)
    # dpkg-source drops *.orig files when unpacking, which would break cargo's
    # checksum check, so remove them (and their checksum entries) up front.
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
    # -d -nc: build-deps and the clean step are Launchpad's job, not this machine's
    (cd "$SRC" && dpkg-buildpackage -S -sa -d -nc "${SIGN[@]}")
    rm -rf "$SRC"
    if [ -n "$PPA" ]; then
        dput "$PPA" "$WORK/zestbay_$VERSION-0ppa${REV}~${series}_source.changes"
    fi
done

echo "Done. Source packages are in $WORK"
