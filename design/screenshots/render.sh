#!/usr/bin/env bash
# Render ZestBay screenshots (README, Flatpak metainfo) from the real QML with
# a mock controller: no PipeWire, no running instance needed.
# Needs qml6 (Qt 6 qml tool) and ImageMagick (magick).
#
#   design/screenshots/render.sh [output dir]      # default: docs/screenshots
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${1:-$ROOT/docs/screenshots}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# A throwaway "ZestBay" module: the real qml/ and fonts/ plus the mock.
mkdir -p "$TMP/ZestBay" "$OUT"
ln -s "$ROOT/qml" "$TMP/ZestBay/qml"
ln -s "$ROOT/fonts" "$TMP/ZestBay/fonts"
cp "$ROOT/design/screenshots/MockController.qml" "$TMP/ZestBay/AppController.qml"
printf 'module ZestBay\nAppController 1.0 AppController.qml\n' > "$TMP/ZestBay/qmldir"

# shot <dark|light> <main|DialogFile> <name> <panel colour> [border px]
shot() {
    local mode=$1 target=$2 name=$3 bg=$4 border=${5:-0}
    rm -f "$TMP/$name".*.png
    QT_QUICK_CONTROLS_STYLE=Basic QT_QPA_PLATFORM=offscreen QT_SCALE_FACTOR=2 \
        timeout 30 qml6 -I "$TMP" "$ROOT/design/screenshots/Grab.qml" -- \
        "$mode" "$target" "$TMP/$name" "file://$TMP/ZestBay/qml/" 2>"$TMP/$name.log" || true
    if ! ls "$TMP/$name".*.png >/dev/null 2>&1; then
        echo "  $name: FAILED"; sed 's/^/    /' "$TMP/$name.log" | head -5; return
    fi
    # shellcheck disable=SC2046
    magick $(ls "$TMP/$name".*.png | sort -V) -append +repage -background "$bg" -flatten \
        -bordercolor "$bg" -border "$border" -alpha off -strip "$OUT/$name.png"
    echo "  $OUT/$name.png"
}

DARK='#1a1d23'
LIGHT='#f6f7f8'
echo "Rendering screenshots:"
shot dark  main          main-dark        "$DARK"
shot light main          main-light       "$LIGHT"
# Dialog content is captured without the window's padding; add it back.
shot dark  RuleEditor    rules            "$DARK" 48
shot dark  PluginBrowser add-plugin       "$DARK" 48
shot dark  PluginParams  parameters       "$DARK" 48
shot dark  CpuOverlay    cpu-usage        "$DARK" 48
shot light Preferences   preferences      "$LIGHT" 48
