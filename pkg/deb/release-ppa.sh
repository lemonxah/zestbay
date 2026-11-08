#!/bin/bash
set -euo pipefail

# Build, sign and upload ZestBay to the Launchpad PPA in one go.
# Wraps pkg/deb/build-ppa.sh and adds signature checks and the upload.
#
# Usage:
#   pkg/deb/release-ppa.sh [--rev N] [--force] [--no-upload] [--dry-run] [SERIES...]
#
#   --rev N       Debian revision (default 1). Use N+1 to re-upload the same
#                 upstream version after Launchpad accepted a broken package.
#   --force       Tell dput to upload again even if it already did (e.g. after
#                 a rejected upload). Not needed for a new version or --rev.
#   --no-upload   Only build, sign and verify.
#   --dry-run     Print what would run, change nothing.
#   SERIES        Ubuntu series, default: noble resolute
#
# Override with env vars: ZESTBAY_GPG_KEY (fingerprint), ZESTBAY_PPA.

KEY="${ZESTBAY_GPG_KEY:-12F24433CE578B2C8195286A042673C3B3528E1D}"
PPA="${ZESTBAY_PPA:-ppa:lemonxah/tools}"
UPLOAD_IMAGE="ubuntu:26.04"   # container used for dput (the local dput is broken)

REV=1 FORCE="" UPLOAD=1 DRY="" SERIES=()
while [ $# -gt 0 ]; do
    case "$1" in
        --rev)       REV="$2"; shift 2 ;;
        --force)     FORCE="--force"; shift ;;
        --no-upload) UPLOAD=""; shift ;;
        --dry-run)   DRY=1; shift ;;
        -h|--help)   sed -n '4,18p' "$0"; exit 0 ;;
        -*)          echo "unknown option $1" >&2; exit 1 ;;
        *)           SERIES+=("$1"); shift ;;
    esac
done
[ ${#SERIES[@]} -gt 0 ] || SERIES=(noble resolute)

PROJECT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$PROJECT_DIR"
OUT="pkg/deb/ppa"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

run() { if [ -n "$DRY" ]; then echo "+ $*"; else "$@"; fi; }
die() { echo "error: $*" >&2; exit 1; }

echo "ZestBay $VERSION (revision $REV) -> $PPA for: ${SERIES[*]}"

# --- preflight -------------------------------------------------------------
for tool in cargo dpkg-buildpackage gpg python3 xz; do
    command -v "$tool" >/dev/null || die "$tool not found"
done
[ -n "$UPLOAD" ] && { command -v podman >/dev/null || die "podman not found (needed to upload)"; }
gpg --list-secret-keys "$KEY" >/dev/null 2>&1 || die "GPG secret key $KEY not found"

changes_files=()
for s in "${SERIES[@]}"; do
    changes_files+=("zestbay_${VERSION}-0ppa${REV}~${s}_source.changes")
done

# A new orig tarball is made per version; refuse to reuse stale same-revision
# signed files silently (Launchpad would reject a duplicate anyway).
if [ -z "$FORCE" ] && [ -z "$DRY" ]; then
    for c in "${changes_files[@]}"; do
        up="$OUT/${c%.changes}.ppa.upload"
        [ -f "$up" ] && die "$c was already uploaded (see $up). Bump the version, use --rev N+1, or --force if it was rejected."
    done
fi

# --- build and sign ---------------------------------------------------------
run pkg/deb/build-ppa.sh --key "$KEY" --rev "$REV" "${SERIES[@]}"

# --- verify -----------------------------------------------------------------
if [ -z "$DRY" ]; then
    for c in "${changes_files[@]}"; do
        [ -f "$OUT/$c" ] || die "missing $OUT/$c"
        gpg --verify "$OUT/$c" 2>&1 | grep -q "Good signature" || die "bad or missing signature on $c"
        echo "signature OK: $c"
    done
fi

[ -n "$UPLOAD" ] || { echo "Built and signed; not uploading (--no-upload)."; exit 0; }

# --- upload -----------------------------------------------------------------
# -u: skip dput's own signature check (the container has no key); Launchpad verifies.
run podman run --rm -v "$PWD/$OUT:/ppa:Z" "$UPLOAD_IMAGE" bash -c \
    "apt-get update -qq && apt-get install -y -qq dput >/dev/null 2>&1 && cd /ppa && dput -u $FORCE $PPA ${changes_files[*]}"

cat <<MSG

Uploaded. Next:
  - Watch your email for Launchpad's accept/reject notices.
  - Builds: https://launchpad.net/~$(echo "${PPA#ppa:}" | cut -d/ -f1)/+archive/ubuntu/$(echo "${PPA#ppa:}" | cut -d/ -f2)/+packages
  - Once "Published", test: sudo apt update && sudo apt install zestbay
MSG
