#!/bin/bash
set -euo pipefail

# Build an .rpm package for ZestBay
# Usage: ./build-rpm.sh
#
# Prerequisites (Fedora):
#   sudo dnf install rust cargo clang cmake pkg-config rpm-build
#     pipewire-devel qt6-qtbase-devel qt6-qtdeclarative-devel
#     lilv-devel lv2-devel suil-devel dbus-devel

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$PROJECT_DIR"

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "Building ZestBay v${VERSION} .rpm..."

# Create rpmbuild directory structure
RPMBUILD_DIR="$SCRIPT_DIR/rpmbuild"
mkdir -p "$RPMBUILD_DIR"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

# Create source tarball
TARBALL_DIR="zestbay-${VERSION}"
git archive --format=tar.gz --prefix="${TARBALL_DIR}/" HEAD \
    > "$RPMBUILD_DIR/SOURCES/zestbay-${VERSION}.tar.gz"

# Copy spec file
cp "$SCRIPT_DIR/zestbay.spec" "$RPMBUILD_DIR/SPECS/"

# Build the RPM
rpmbuild --define "_topdir $RPMBUILD_DIR" -bb "$RPMBUILD_DIR/SPECS/zestbay.spec"

echo ""
echo "RPM built in: $RPMBUILD_DIR/RPMS/"
find "$RPMBUILD_DIR/RPMS/" -name "*.rpm" -print
