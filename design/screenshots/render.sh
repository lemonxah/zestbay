#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${1:-$ROOT/docs/screenshots}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/ZestBay" "$OUT"
ln -s "$ROOT/qml" "$TMP/ZestBay/qml"
ln -s "$ROOT/fonts" "$TMP/ZestBay/fonts"
cp "$ROOT/design/screenshots/MockController.qml" "$TMP/ZestBay/AppController.qml"
printf 'module ZestBay\nAppController 1.0 AppController.qml\n' > "$TMP/ZestBay/qmldir"

look() {
    sed -i -e "s/string palette: \".*\"/string palette: \"$1\"/" \
        -e "s/string wireStyle: \".*\"/string wireStyle: \"$2\"/" "$TMP/ZestBay/AppController.qml"
}

shot() {
    local mode=$1 target=$2 name=$3 bg=$4 border=${5:-0}
    rm -f "$TMP/$name".*.png
    QT_QUICK_CONTROLS_STYLE=Basic QT_QPA_PLATFORM=offscreen QT_SCALE_FACTOR=2 \
        timeout 30 qml6 -I "$TMP" "$ROOT/design/screenshots/Grab.qml" -- \
        "$mode" "$target" "$TMP/$name" "file://$TMP/ZestBay/qml/" 2>"$TMP/$name.log" || true
    if ! ls "$TMP/$name".*.png >/dev/null 2>&1; then
        echo "  $name: FAILED"; sed 's/^/    /' "$TMP/$name.log" | head -5; return
    fi
    magick $(ls "$TMP/$name".*.png | sort -V) -append +repage -background "$bg" -flatten \
        -bordercolor "$bg" -border "$border" -alpha off -strip "$OUT/$name.png"
    echo "  $OUT/$name.png"
}

DARK='#1a1d23'
LIGHT='#f6f7f8'
echo "Rendering screenshots:"
shot dark  main          main-dark        "$DARK"
shot light main          main-light       "$LIGHT"
shot dark  main-presets  presets          "$DARK"
[ -f "$OUT/presets.png" ] && magick "$OUT/presets.png" -crop 640x1180+1620+270 +repage "$OUT/presets.png"
shot dark  main-sessions sessions         "$DARK"
shot dark  main-portmenu port-menu        "$DARK"
[ -f "$OUT/sessions.png" ] && magick "$OUT/sessions.png" -crop 1180x430+0+0 +repage "$OUT/sessions-crop.png"
[ -f "$OUT/port-menu.png" ] && magick "$OUT/port-menu.png" -crop 700x640+1700+300 +repage "$OUT/port-menu-crop.png"
shot dark  main-metermenu  meter-menu     "$DARK"
shot dark  main-nodemeters node-meters    "$DARK"
[ -f "$OUT/main-dark.png" ] && magick "$OUT/main-dark.png" -crop 1500x1150+110+195 +repage "$OUT/meters-crop.png"
[ -f "$OUT/meter-menu.png" ] && magick "$OUT/meter-menu.png" -crop 890x1075+1385+315 +repage "$OUT/meter-menu-crop.png"
[ -f "$OUT/node-meters.png" ] && magick "$OUT/node-meters.png" -crop 860x540+1165+200 +repage "$OUT/node-meters-crop.png"
shot dark  RuleEditor    rules            "$DARK" 48
shot dark  PluginBrowser add-plugin       "$DARK" 48
shot dark  PluginParams  parameters       "$DARK" 48
shot dark  CpuOverlay    cpu-usage        "$DARK" 48
shot light Preferences   preferences      "$LIGHT" 48
for palette in catppuccin gruvbox nord; do
    look "$palette" circuit
    shot dark main "theme-$palette" "$DARK"
done
look solarized circuit
shot light main          theme-solarized  "$LIGHT"
look zest circuit
shot dark  main          wires-circuit    "$DARK"
look zest bezier
if [ -f "$OUT/theme-catppuccin.png" ] && [ -f "$OUT/theme-gruvbox.png" ] && [ -f "$OUT/theme-nord.png" ] && [ -f "$OUT/theme-solarized.png" ]; then
    tile=(-crop 1240x900+640+200 +repage -resize 800x)
    magick \( "$OUT/theme-catppuccin.png" "$OUT/theme-gruvbox.png" "${tile[@]}" +append \) \
        \( "$OUT/theme-nord.png" "$OUT/theme-solarized.png" "${tile[@]}" +append \) -append +repage -strip "$OUT/themes.png"
    rm "$OUT"/theme-{catppuccin,gruvbox,nord,solarized}.png
fi
if [ -f "$OUT/wires-circuit.png" ]; then
    magick "$OUT/wires-circuit.png" -crop 2050x1200+620+190 +repage "$OUT/wires-crop.png"
    rm "$OUT/wires-circuit.png"
fi
