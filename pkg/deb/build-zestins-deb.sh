#!/bin/bash
set -euo pipefail

# Build a .deb package for Zestins (ZestBay's LV2 plugins, zestins/)
# Usage:
#   ./build-zestins-deb.sh                      build on this Debian/Ubuntu machine;
#                                               on other distros, same as --container
#   ./build-zestins-deb.sh --container [IMAGE]  build inside IMAGE (default ubuntu:24.04)
#                                               with podman or docker, from any distro
#
# The .deb only works on the release it was built on (or newer), so build one
# per target, e.g. --container ubuntu:24.04, --container ubuntu:25.04.
#
# Prerequisites for a native build (Debian/Ubuntu):
#   sudo apt install build-essential pkg-config dpkg-dev lv2-dev libx11-dev

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
    # Same image as the ZestBay .deb (it has everything Zestins needs too)
    TAG="zestbay-deb-build:${IMAGE//[:\/]/-}"
    "$ENGINE" build --build-arg "BASE=$IMAGE" -t "$TAG" -f "$SCRIPT_DIR/Containerfile" "$SCRIPT_DIR"
    USER_ARGS=()
    [[ "$ENGINE" == *docker ]] && USER_ARGS=(--user "$(id -u):$(id -g)")
    "$ENGINE" run --rm "${USER_ARGS[@]}" \
        -v "$PROJECT_DIR:/src:Z" -w /src \
        -e ZESTINS_BUILD="/src/target/zestins-deb-${IMAGE//[:\/]/-}" \
        -e DEB_SUFFIX="${IMAGE//[:\/]/}" \
        "$TAG" pkg/deb/build-zestins-deb.sh
    exit 0
fi

# Zestins ships with ZestBay's version
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
ARCH=$(dpkg --print-architecture)
PKG_NAME="zestins"
BUILD_DIR="${ZESTINS_BUILD:-$PROJECT_DIR/target/zestins-deb}"
DEB_VERSION="${VERSION}${DEB_SUFFIX:+-$DEB_SUFFIX}"
PKG_DIR="$SCRIPT_DIR/${PKG_NAME}_${DEB_VERSION}_${ARCH}"

echo "Building Zestins v${DEB_VERSION} .deb for ${ARCH}..."

# A fresh build, so the plugins are built for this system
rm -rf "$BUILD_DIR"
make -C zestins BUILD="$BUILD_DIR"

rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN"
make -C zestins BUILD="$BUILD_DIR" install DESTDIR="$PKG_DIR" LV2_DIR=/usr/lib/lv2
strip --strip-unneeded "$PKG_DIR"/usr/lib/lv2/zestins.lv2/*.so
install -Dm644 "LICENSE" "$PKG_DIR/usr/share/doc/zestins/copyright"

# Shared-library dependencies, worked out from the plugins' actual links.
# dpkg-shlibdeps insists on a debian/control, so give it one in a scratch dir.
SHLIBS_DIR=$(mktemp -d)
trap 'rm -rf "$SHLIBS_DIR"' EXIT
mkdir -p "$SHLIBS_DIR/debian"
printf 'Source: %s\n\nPackage: %s\nArchitecture: any\n' "$PKG_NAME" "$PKG_NAME" > "$SHLIBS_DIR/debian/control"
SHLIBS_DEPENDS=$(cd "$SHLIBS_DIR" && dpkg-shlibdeps -O "$PKG_DIR"/usr/lib/lv2/zestins.lv2/*.so \
    | sed -n 's/^shlibs:Depends=//p')

INSTALLED_SIZE=$(du -sk "$PKG_DIR" | cut -f1)

cat > "$PKG_DIR/DEBIAN/control" <<EOF
Package: zestins
Version: ${DEB_VERSION}
Section: sound
Priority: optional
Architecture: ${ARCH}
Depends: ${SHLIBS_DEPENDS}
Enhances: zestbay
Provides: lv2-plugin
Installed-Size: ${INSTALLED_SIZE}
Maintainer: Ryno Kotze <lemon.xah@gmail.com>
Homepage: https://github.com/lemonxah/zestbay
Description: LV2 plugins from ZestBay
 Zestins are the LV2 plugins that come with ZestBay: utility (gain,
 balance, mute, polarity, mono), meter, EQ, gate, compressor, delay,
 reverb, tone generator, a simple synth, a MIDI switch and a MIDI utility.
 Each has an X11 UI and works in any LV2 host.
EOF

dpkg-deb --build --root-owner-group "$PKG_DIR"
rm -rf "$PKG_DIR"

echo ""
echo "Package built: ${PKG_DIR}.deb"
