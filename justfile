# Get current version from Cargo.toml
version := `grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'`

# Show current version
current:
    @echo "{{version}}"

# Bump version: just bump 0.3.0
bump new_version msg="":
    # Cargo.toml + Cargo.lock
    sed -i '0,/^version = ".*"/s//version = "{{new_version}}"/' Cargo.toml
    cargo update --workspace
    # Arch PKGBUILD
    sed -i 's/^pkgver=.*/pkgver={{new_version}}/' pkg/zestbay/PKGBUILD
    # RPM spec
    sed -i 's/^Version:.*/Version:        {{new_version}}/' pkg/rpm/zestbay.spec
    # Flatpak/AppStream metainfo: what Flathub and software centres show
    just metainfo-release {{new_version}} {{quote(msg)}}
    @echo "Bumped to {{new_version}}"

# Add a dated <release> to the Flatpak metainfo (skips a version already listed)
metainfo-release version msg="":
    #!/usr/bin/env python3
    import datetime, html, re, sys
    from pathlib import Path
    path = Path("pkg/flatpak/io.github.lemonxah.ZestBay.metainfo.xml")
    text = path.read_text()
    version, msg = {{quote(version)}}, {{quote(msg)}}
    if re.search(r'<release\s+version="' + re.escape(version) + '"', text):
        print(f"metainfo: {version} already listed")
        sys.exit(0)
    m = re.search(r"^([ \t]*)<releases>[ \t]*$", text, re.M)
    if not m:
        sys.exit("metainfo: no <releases> element")
    pad = m.group(1) + "  "
    date = datetime.date.today().isoformat()
    if msg:
        entry = (f'{pad}<release version="{version}" date="{date}">\n'
                 f'{pad}  <description>\n{pad}    <p>{html.escape(msg)}</p>\n{pad}  </description>\n'
                 f'{pad}</release>\n')
    else:
        entry = f'{pad}<release version="{version}" date="{date}"/>\n'
    at = text.index("\n", m.end()) + 1
    path.write_text(text[:at] + entry + text[at:])
    print(f"metainfo: added release {version} ({date})")

# Run all tests
test:
    cargo test --workspace

# Build release binary
build:
    cargo build --workspace --release

# Build and install locally
install: build
    sudo install -Dm755 target/release/zestbay /usr/bin/zestbay
    sudo install -Dm755 target/release/zestbay-ui-bridge /usr/bin/zestbay-ui-bridge
    sudo install -Dm644 zestbay.desktop /usr/share/applications/zestbay.desktop
    sudo install -Dm644 images/logo/zestbay-launcher.svg /usr/share/icons/hicolor/scalable/apps/zestbay.svg
    sudo install -Dm644 images/zesticon.png /usr/share/icons/hicolor/256x256/apps/zestbay.png
    sudo install -Dm644 images/zesttray.png /usr/share/icons/hicolor/256x256/apps/zestbay-tray.png
    -sudo gtk-update-icon-cache -q /usr/share/icons/hicolor
    -kbuildsycoca6 --noincremental >/dev/null 2>&1
    @echo "Installed"
    @echo "KDE keeps loaded icons in memory: run 'systemctl --user restart plasma-plasmashell' (or log out and in) to see a new icon."

# Bump, commit, tag, and push. Optional message: just release 0.5.0 "fix remapping"
release new_version msg="":
    just bump {{new_version}} {{quote(msg)}}
    git add Cargo.toml Cargo.lock pkg/zestbay/PKGBUILD pkg/rpm/zestbay.spec pkg/flatpak/io.github.lemonxah.ZestBay.metainfo.xml
    git commit -m "v{{new_version}}{{ if msg != "" { ": " + msg } else { "" } }}"
    git tag -a "v{{new_version}}" -m "v{{new_version}}{{ if msg != "" { ": " + msg } else { "" } }}"
    git push && git push --tags
    gh release create "v{{new_version}}" --generate-notes

# ---- Flatpak (local builds; follows docs.flathub.org "Submission") ----------

flatpak_manifest := "io.github.lemonxah.ZestBay.yml"
flatpak_id := "io.github.lemonxah.ZestBay"

# One-time: Flathub's builder (flatpak-builder + linter) and the user Flathub remote
flatpak-setup:
    flatpak install --user -y flathub org.flatpak.Builder
    flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo

# Regenerate the offline crate list cargo-sources.json (next to the manifest) from Cargo.lock
flatpak-sources:
    python3 pkg/flatpak/cargo-sources.py

# Build like Flathub does (flathub-build) and install it for this user
flatpak: flatpak-sources
    flatpak run --command=flathub-build org.flatpak.Builder --install {{flatpak_manifest}}

# Run the installed Flatpak
flatpak-run:
    flatpak run {{flatpak_id}}

# Metainfo, desktop file and icons stay upstream, as Flathub requires; this only
# copies files (no git, nothing sent to Flathub).
# Copy the Flathub submission files (manifest, cargo-sources.json, flathub.json if present) into a folder
flatpak-copy dest: flatpak-sources
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{dest}}"
    cp -v {{flatpak_manifest}} cargo-sources.json "{{dest}}/"
    if [ -f flathub.json ]; then cp -v flathub.json "{{dest}}/"; fi

# Users install it with `flatpak install --user dist/ZestBay-<version>.flatpak`
# (or by opening it in Discover / GNOME Software); the KDE runtime comes from
# Flathub automatically. Attach it to a GitHub release by hand.
# Build, then pack a single-file bundle: dist/ZestBay-<version>.flatpak
flatpak-bundle: flatpak
    mkdir -p dist
    flatpak build-bundle repo "dist/ZestBay-{{version}}.flatpak" {{flatpak_id}} master \
        --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
    @ls -lh "dist/ZestBay-{{version}}.flatpak"

# Flathub's linter on the manifest and on the built repo (run `just flatpak` first)
flatpak-lint:
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest {{flatpak_manifest}}
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo

# ---- Debian/Ubuntu ----------------------------------------------------------

# Works on that Ubuntu release and newer; build one per target, e.g. just deb ubuntu:25.04
# Build a .deb inside an Ubuntu container (podman/docker): pkg/deb/zestbay_<version>-<image>_amd64.deb
deb image="ubuntu:24.04":
    pkg/deb/build-deb.sh --container {{image}}
