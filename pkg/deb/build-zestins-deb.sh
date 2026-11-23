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
        -e ZESTINS_BUILD="/src/target/zestins-deb-${IMAGE//[:\/]/-}" \
        -e DEB_SUFFIX="${IMAGE//[:\/]/}" \
        "$TAG" pkg/deb/build-zestins-deb.sh
    exit 0
fi

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
ARCH=$(dpkg --print-architecture)
PKG_NAME="zestins"
BUILD_DIR="${ZESTINS_BUILD:-$PROJECT_DIR/target/zestins-deb}"
DEB_VERSION="${VERSION}${DEB_SUFFIX:+-$DEB_SUFFIX}"
PKG_DIR="$SCRIPT_DIR/${PKG_NAME}_${DEB_VERSION}_${ARCH}"

echo "Building Zestins v${DEB_VERSION} .deb for ${ARCH}..."

rm -rf "$BUILD_DIR"
make -C zestins BUILD="$BUILD_DIR"

rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN"
make -C zestins BUILD="$BUILD_DIR" install DESTDIR="$PKG_DIR" LV2_DIR=/usr/lib/lv2
strip --strip-unneeded "$PKG_DIR"/usr/lib/lv2/zestins.lv2/*.so
install -Dm644 "LICENSE" "$PKG_DIR/usr/share/doc/zestins/copyright"

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
