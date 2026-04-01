#!/bin/bash
set -euo pipefail

# Build a .deb package for ZestBay
# Usage: ./build-deb.sh [--release]
#
# Prerequisites (Debian/Ubuntu):
#   sudo apt install build-essential pkg-config cmake clang
#     libpipewire-0.3-dev qt6-base-dev qt6-declarative-dev
#     liblilv-dev lv2-dev libsuil-dev libdbus-1-dev

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$PROJECT_DIR"

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
ARCH=$(dpkg --print-architecture 2>/dev/null || echo amd64)
PKG_NAME="zestbay"
PKG_DIR="$SCRIPT_DIR/${PKG_NAME}_${VERSION}_${ARCH}"

echo "Building ZestBay v${VERSION} .deb for ${ARCH}..."

# Build the release binary
export RUSTUP_TOOLCHAIN=stable
cargo build --workspace --release

# Clean and create package structure
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN"
mkdir -p "$PKG_DIR/usr/bin"
mkdir -p "$PKG_DIR/usr/lib/zestbay"
mkdir -p "$PKG_DIR/usr/share/applications"
mkdir -p "$PKG_DIR/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$PKG_DIR/usr/share/licenses/zestbay"

# Install files
install -m755 "target/release/$PKG_NAME" "$PKG_DIR/usr/bin/$PKG_NAME"
install -m755 "target/release/zestbay-ui-bridge" "$PKG_DIR/usr/lib/zestbay/zestbay-ui-bridge"
install -m644 "zestbay.desktop" "$PKG_DIR/usr/share/applications/zestbay.desktop"
install -m644 "images/zesticon.png" "$PKG_DIR/usr/share/icons/hicolor/256x256/apps/zestbay.png"
install -m644 "images/zesttray.png" "$PKG_DIR/usr/share/icons/hicolor/256x256/apps/zestbay-tray.png"
install -m644 "LICENSE" "$PKG_DIR/usr/share/licenses/zestbay/LICENSE"

# Calculate installed size
INSTALLED_SIZE=$(du -sk "$PKG_DIR" | cut -f1)

# Write control file
cat > "$PKG_DIR/DEBIAN/control" <<EOF
Package: zestbay
Version: ${VERSION}
Section: sound
Priority: optional
Architecture: ${ARCH}
Depends: pipewire (>= 0.3), libqt6core6t64 | libqt6core6, libqt6gui6t64 | libqt6gui6, libqt6qml6, libqt6quick6, liblilv-0-0, libx11-6, libdbus-1-3
Recommends: libsuil-0-0
Installed-Size: ${INSTALLED_SIZE}
Maintainer: Ryno Kotze <lemon.xah@gmail.com>
Homepage: https://github.com/lemonxah/zestbay
Description: PipeWire patchbay and audio routing manager
 ZestBay is a PipeWire patchbay application with LV2, CLAP, and VST3
 plugin hosting, MIDI mapping, and a visual node-graph editor for
 routing audio and MIDI between applications and devices.
EOF

# Build the .deb
dpkg-deb --build --root-owner-group "$PKG_DIR"

echo ""
echo "Package built: ${PKG_DIR}.deb"
