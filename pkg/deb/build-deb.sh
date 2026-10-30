#!/bin/bash
set -euo pipefail

# Build a .deb package for ZestBay
# Usage:
#   ./build-deb.sh                          build on this Debian/Ubuntu machine; on
#                                           other distros, same as --container
#   ./build-deb.sh --container [IMAGE]      build inside IMAGE (default ubuntu:24.04)
#                                           with podman or docker, from any distro
#
# The .deb only works on the release it was built on (or newer), so build one
# per target, e.g. --container ubuntu:24.04, --container ubuntu:25.04.
#
# Prerequisites for a native build (Debian/Ubuntu):
#   sudo apt install build-essential pkg-config cmake clang libclang-dev dpkg-dev
#     libpipewire-0.3-dev qt6-base-dev qt6-base-private-dev qt6-declarative-dev
#     qt6-declarative-private-dev liblilv-dev lv2-dev libsuil-dev libdbus-1-dev
#     libx11-dev libgtk-3-dev
#   plus Rust >= 1.85 from rustup (Ubuntu's packaged rustc is too old)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$PROJECT_DIR"

# A .deb built against a non-Debian host's libraries won't run on Ubuntu (and
# dpkg-shlibdeps can't map them to packages), so build in a container there.
if [ "${1:-}" != "--container" ] && [ ! -f /etc/debian_version ]; then
    echo "Not a Debian/Ubuntu system: building in an ubuntu:24.04 container instead"
    set -- --container
fi

if [ "${1:-}" = "--container" ]; then
    IMAGE="${2:-ubuntu:24.04}"
    ENGINE=$(command -v podman || command -v docker || true)
    [ -n "$ENGINE" ] || { echo "podman or docker is required for --container" >&2; exit 1; }
    TAG="zestbay-deb-build:${IMAGE//[:\/]/-}"
    "$ENGINE" build --build-arg "BASE=$IMAGE" -t "$TAG" -f "$SCRIPT_DIR/Containerfile" "$SCRIPT_DIR"
    # Separate target dir so the host's build cache isn't mixed with the container's.
    # Rootless podman maps root to the calling user; docker needs --user so the
    # output isn't owned by root.
    USER_ARGS=()
    [[ "$ENGINE" == *docker ]] && USER_ARGS=(--user "$(id -u):$(id -g)")
    "$ENGINE" run --rm "${USER_ARGS[@]}" \
        -v "$PROJECT_DIR:/src:Z" -w /src \
        -e CARGO_TARGET_DIR="/src/target/deb-${IMAGE//[:\/]/-}" \
        -e DEB_SUFFIX="${IMAGE//[:\/]/}" \
        "$TAG" pkg/deb/build-deb.sh
    exit 0
fi

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
ARCH=$(dpkg --print-architecture)
PKG_NAME="zestbay"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
# e.g. zestbay_1.1.2_amd64 or zestbay_1.1.2-ubuntu24.04_amd64
DEB_VERSION="${VERSION}${DEB_SUFFIX:+-$DEB_SUFFIX}"
PKG_DIR="$SCRIPT_DIR/${PKG_NAME}_${DEB_VERSION}_${ARCH}"

echo "Building ZestBay v${DEB_VERSION} .deb for ${ARCH}..."

# Build the release binary
cargo build --workspace --release --locked

# Clean and create package structure
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN"

# Install files
install -Dm755 "$TARGET_DIR/release/$PKG_NAME" "$PKG_DIR/usr/bin/$PKG_NAME"
install -Dm755 "$TARGET_DIR/release/zestbay-ui-bridge" "$PKG_DIR/usr/lib/zestbay/zestbay-ui-bridge"
strip "$PKG_DIR/usr/bin/$PKG_NAME" "$PKG_DIR/usr/lib/zestbay/zestbay-ui-bridge"
install -Dm644 "zestbay.desktop" "$PKG_DIR/usr/share/applications/zestbay.desktop"
install -Dm644 "images/logo/zestbay-launcher.svg" "$PKG_DIR/usr/share/icons/hicolor/scalable/apps/zestbay.svg"
install -Dm644 "images/zesticon.png" "$PKG_DIR/usr/share/icons/hicolor/256x256/apps/zestbay.png"
install -Dm644 "images/zesttray.png" "$PKG_DIR/usr/share/icons/hicolor/256x256/apps/zestbay-tray.png"
install -Dm644 "LICENSE" "$PKG_DIR/usr/share/doc/zestbay/copyright"

# Shared-library dependencies, worked out from the binaries' actual links.
# dpkg-shlibdeps insists on a debian/control, so give it one in a scratch dir.
SHLIBS_DIR=$(mktemp -d)
trap 'rm -rf "$SHLIBS_DIR"' EXIT
mkdir -p "$SHLIBS_DIR/debian"
printf 'Source: %s\n\nPackage: %s\nArchitecture: any\n' "$PKG_NAME" "$PKG_NAME" > "$SHLIBS_DIR/debian/control"
SHLIBS_DEPENDS=$(cd "$SHLIBS_DIR" && dpkg-shlibdeps -O \
    "$PKG_DIR/usr/bin/$PKG_NAME" "$PKG_DIR/usr/lib/zestbay/zestbay-ui-bridge" \
    | sed -n 's/^shlibs:Depends=//p')

# QML modules are loaded at runtime, so dpkg-shlibdeps can't see them
QML_DEPENDS="qml6-module-qtquick, qml6-module-qtquick-controls, qml6-module-qtquick-layouts, qml6-module-qtquick-shapes, qml6-module-qtquick-templates, qml6-module-qtquick-window, qml6-module-qtqml-models, qml6-module-qtqml-workerscript"

# Calculate installed size
INSTALLED_SIZE=$(du -sk "$PKG_DIR" | cut -f1)

# Write control file
cat > "$PKG_DIR/DEBIAN/control" <<EOF
Package: zestbay
Version: ${DEB_VERSION}
Section: sound
Priority: optional
Architecture: ${ARCH}
Depends: ${SHLIBS_DEPENDS}, ${QML_DEPENDS}, pipewire (>= 0.3)
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
rm -rf "$PKG_DIR"

echo ""
echo "Package built: ${PKG_DIR}.deb"
