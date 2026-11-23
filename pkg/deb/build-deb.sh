#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$PROJECT_DIR"

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
DEB_VERSION="${VERSION}${DEB_SUFFIX:+-$DEB_SUFFIX}"
PKG_DIR="$SCRIPT_DIR/${PKG_NAME}_${DEB_VERSION}_${ARCH}"

echo "Building ZestBay v${DEB_VERSION} .deb for ${ARCH}..."

cargo build --workspace --release --locked

rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN"

install -Dm755 "$TARGET_DIR/release/$PKG_NAME" "$PKG_DIR/usr/bin/$PKG_NAME"
install -Dm755 "$TARGET_DIR/release/zestbay-ui-bridge" "$PKG_DIR/usr/lib/zestbay/zestbay-ui-bridge"
strip "$PKG_DIR/usr/bin/$PKG_NAME" "$PKG_DIR/usr/lib/zestbay/zestbay-ui-bridge"
install -Dm644 "zestbay.desktop" "$PKG_DIR/usr/share/applications/zestbay.desktop"
install -Dm644 "images/logo/zestbay-launcher.svg" "$PKG_DIR/usr/share/icons/hicolor/scalable/apps/zestbay.svg"
install -Dm644 "images/zesticon.png" "$PKG_DIR/usr/share/icons/hicolor/256x256/apps/zestbay.png"
install -Dm644 "images/zesttray.png" "$PKG_DIR/usr/share/icons/hicolor/256x256/apps/zestbay-tray.png"
install -Dm644 "LICENSE" "$PKG_DIR/usr/share/doc/zestbay/copyright"

SHLIBS_DIR=$(mktemp -d)
trap 'rm -rf "$SHLIBS_DIR"' EXIT
mkdir -p "$SHLIBS_DIR/debian"
printf 'Source: %s\n\nPackage: %s\nArchitecture: any\n' "$PKG_NAME" "$PKG_NAME" > "$SHLIBS_DIR/debian/control"
SHLIBS_DEPENDS=$(cd "$SHLIBS_DIR" && dpkg-shlibdeps -O \
    "$PKG_DIR/usr/bin/$PKG_NAME" "$PKG_DIR/usr/lib/zestbay/zestbay-ui-bridge" \
    | sed -n 's/^shlibs:Depends=//p')

QML_DEPENDS="qml6-module-qtquick, qml6-module-qtquick-controls, qml6-module-qtquick-layouts, qml6-module-qtquick-shapes, qml6-module-qtquick-templates, qml6-module-qtquick-window, qml6-module-qtqml-models, qml6-module-qtqml-workerscript"

INSTALLED_SIZE=$(du -sk "$PKG_DIR" | cut -f1)

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

dpkg-deb --build --root-owner-group "$PKG_DIR"
rm -rf "$PKG_DIR"

echo ""
echo "Package built: ${PKG_DIR}.deb"
