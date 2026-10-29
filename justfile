# Get current version from Cargo.toml
version := `grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'`

# Show current version
current:
    @echo "{{version}}"

# Bump version: just bump 0.3.0
bump new_version:
    # Cargo.toml + Cargo.lock
    sed -i '0,/^version = ".*"/s//version = "{{new_version}}"/' Cargo.toml
    cargo update --workspace
    # Arch PKGBUILD
    sed -i 's/^pkgver=.*/pkgver={{new_version}}/' pkg/zestbay/PKGBUILD
    # RPM spec
    sed -i 's/^Version:.*/Version:        {{new_version}}/' pkg/rpm/zestbay.spec
    @echo "Bumped to {{new_version}}"

# Run all tests
test:
    cargo test --workspace

# Build release binary
build:
    cargo build --workspace --release

# Build and install locally
install: build
    sudo cp target/release/zestbay /usr/bin/zestbay
    sudo cp target/release/zestbay-ui-bridge /usr/lib/zestbay/zestbay-ui-bridge
    sudo install -Dm644 images/zesticon.png /usr/share/icons/hicolor/256x256/apps/zestbay.png
    sudo install -Dm644 images/zesttray.png /usr/share/icons/hicolor/256x256/apps/zestbay-tray.png
    -sudo gtk-update-icon-cache -q /usr/share/icons/hicolor
    @echo "Installed"

# Bump, commit, tag, and push. Optional message: just release 0.5.0 "fix remapping"
release new_version msg="":
    just bump {{new_version}}
    git add Cargo.toml Cargo.lock pkg/zestbay/PKGBUILD pkg/rpm/zestbay.spec
    git commit -m "v{{new_version}}{{ if msg != "" { ": " + msg } else { "" } }}"
    git tag -a "v{{new_version}}" -m "v{{new_version}}{{ if msg != "" { ": " + msg } else { "" } }}"
    git push && git push --tags
    gh release create "v{{new_version}}" --generate-notes

# ---- Flatpak (local builds; follows docs.flathub.org "Submission") ----------

flatpak_manifest := "pkg/flatpak/io.github.lemonxah.ZestBay.yml"
flatpak_id := "io.github.lemonxah.ZestBay"

# One-time: Flathub's builder (flatpak-builder + linter) and the user Flathub remote
flatpak-setup:
    flatpak install --user -y flathub org.flatpak.Builder
    flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo

# Regenerate the offline crate list pkg/flatpak/cargo-sources.json from Cargo.lock
flatpak-sources:
    #!/usr/bin/env bash
    set -euo pipefail
    tools=.flatpak-tools
    mkdir -p "$tools"
    if [ ! -x "$tools/venv/bin/python" ]; then
        python3 -m venv "$tools/venv"
        "$tools/venv/bin/pip" install -q aiohttp tomlkit
    fi
    if [ ! -f "$tools/flatpak-cargo-generator.py" ]; then
        curl -fsSL -o "$tools/flatpak-cargo-generator.py" \
            https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
    fi
    "$tools/venv/bin/python" -I "$tools/flatpak-cargo-generator.py" Cargo.lock -o pkg/flatpak/cargo-sources.json

# Build like Flathub does (flathub-build) and install it for this user
flatpak: flatpak-sources
    flatpak run --command=flathub-build org.flatpak.Builder --install {{flatpak_manifest}}

# Run the installed Flatpak
flatpak-run:
    flatpak run {{flatpak_id}}

# Flathub's linter on the manifest and on the built repo (run `just flatpak` first)
flatpak-lint:
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest {{flatpak_manifest}}
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo
