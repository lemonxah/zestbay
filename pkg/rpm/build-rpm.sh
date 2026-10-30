#!/bin/bash
set -euo pipefail

# Build an .rpm package for ZestBay
# Usage:
#   ./build-rpm.sh                          build on this Fedora machine; on
#                                           other distros, same as --container
#   ./build-rpm.sh --container [IMAGE]      build inside IMAGE (default fedora:latest)
#                                           with podman or docker, from any distro
#
# The .rpm only works on the Fedora release it was built for, so build one per
# target, e.g. --container fedora:42, --container fedora:43.
#
# Prerequisites for a native build (Fedora):
#   sudo dnf install rpm-build 'dnf-command(builddep)'
#   sudo dnf builddep pkg/rpm/zestbay.spec

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$PROJECT_DIR"

# An .rpm built against a non-Fedora host's libraries won't run on Fedora, so
# build in a container there.
if [ "${1:-}" != "--container" ] && [ ! -f /etc/fedora-release ]; then
    echo "Not a Fedora system: building in a fedora:latest container instead"
    set -- --container
fi

if [ "${1:-}" = "--container" ]; then
    IMAGE="${2:-fedora:latest}"
    ENGINE=$(command -v podman || command -v docker || true)
    [ -n "$ENGINE" ] || { echo "podman or docker is required for --container" >&2; exit 1; }
    TAG="zestbay-rpm-build:${IMAGE//[:\/]/-}"
    # Pick up a new release behind a moving tag like fedora:latest
    PULL_ARG=--pull=newer
    [[ "$ENGINE" == *docker ]] && PULL_ARG=--pull
    "$ENGINE" build "$PULL_ARG" --build-arg "BASE=$IMAGE" -t "$TAG" -f "$SCRIPT_DIR/Containerfile" "$SCRIPT_DIR"
    # Rootless podman maps root to the calling user; docker needs --user so the
    # output isn't owned by root.
    USER_ARGS=()
    [[ "$ENGINE" == *docker ]] && USER_ARGS=(--user "$(id -u):$(id -g)" -e HOME=/tmp)
    "$ENGINE" run --rm "${USER_ARGS[@]}" \
        -v "$PROJECT_DIR:/src:Z" -w /src \
        "$TAG" pkg/rpm/build-rpm.sh
    exit 0
fi

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "Building ZestBay v${VERSION} .rpm..."

# Create rpmbuild directory structure
RPMBUILD_DIR="$SCRIPT_DIR/rpmbuild"
mkdir -p "$RPMBUILD_DIR"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

# Create source tarball from the working tree (build output and packaging
# artifacts left out)
TARBALL_DIR="zestbay-${VERSION}"
tar -czf "$RPMBUILD_DIR/SOURCES/zestbay-${VERSION}.tar.gz" \
    --exclude-vcs \
    --exclude=./target --exclude=./repo --exclude=./.flatpak-builder \
    --exclude=./build-dir --exclude=./builddir --exclude=./dist \
    --exclude=./pkg/rpm/rpmbuild --exclude='./pkg/deb/zestbay_*' \
    --transform "s,^\.,${TARBALL_DIR}," \
    .

# Copy spec file
cp "$SCRIPT_DIR/zestbay.spec" "$RPMBUILD_DIR/SPECS/"

# Build the RPM
rpmbuild --define "_topdir $RPMBUILD_DIR" -bb "$RPMBUILD_DIR/SPECS/zestbay.spec"

echo ""
echo "RPM built in: $RPMBUILD_DIR/RPMS/"
find "$RPMBUILD_DIR/RPMS/" -name "*.rpm" -print
