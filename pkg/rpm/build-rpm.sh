#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$PROJECT_DIR"

if [ "${1:-}" != "--container" ] && [ ! -f /etc/fedora-release ]; then
    echo "Not a Fedora system: building in a fedora:latest container instead"
    set -- --container
fi

if [ "${1:-}" = "--container" ]; then
    IMAGE="${2:-fedora:latest}"
    ENGINE=$(command -v podman || command -v docker || true)
    [ -n "$ENGINE" ] || { echo "podman or docker is required for --container" >&2; exit 1; }
    TAG="zestbay-rpm-build:${IMAGE//[:\/]/-}"
    PULL_ARG=--pull=newer
    [[ "$ENGINE" == *docker ]] && PULL_ARG=--pull
    "$ENGINE" build "$PULL_ARG" --build-arg "BASE=$IMAGE" -t "$TAG" -f "$SCRIPT_DIR/Containerfile" "$SCRIPT_DIR"
    USER_ARGS=()
    [[ "$ENGINE" == *docker ]] && USER_ARGS=(--user "$(id -u):$(id -g)" -e HOME=/tmp)
    "$ENGINE" run --rm "${USER_ARGS[@]}" \
        -v "$PROJECT_DIR:/src:Z" -w /src \
        "$TAG" pkg/rpm/build-rpm.sh
    exit 0
fi

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "Building ZestBay v${VERSION} .rpm..."

RPMBUILD_DIR="$SCRIPT_DIR/rpmbuild"
mkdir -p "$RPMBUILD_DIR"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

TARBALL_DIR="zestbay-${VERSION}"
tar -czf "$RPMBUILD_DIR/SOURCES/zestbay-${VERSION}.tar.gz" \
    --exclude-vcs \
    --exclude=./target --exclude=./repo --exclude=./.flatpak-builder \
    --exclude=./build-dir --exclude=./builddir --exclude=./dist \
    --exclude=./pkg/rpm/rpmbuild --exclude='./pkg/deb/zestbay_*' \
    --exclude=./zestins/build \
    --transform "s,^\.,${TARBALL_DIR}," \
    .

cp "$SCRIPT_DIR/zestbay.spec" "$RPMBUILD_DIR/SPECS/"

rpmbuild --define "_topdir $RPMBUILD_DIR" -bb "$RPMBUILD_DIR/SPECS/zestbay.spec"

echo ""
echo "RPM built in: $RPMBUILD_DIR/RPMS/"
find "$RPMBUILD_DIR/RPMS/" -name "*.rpm" -print
