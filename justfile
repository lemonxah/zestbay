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

# Regenerate pkg/flatpak/cargo-sources.json from Cargo.lock (see pkg/flatpak/README.md)
flatpak-sources:
    flatpak-cargo-generator.py Cargo.lock -o pkg/flatpak/cargo-sources.json

# Build the Flatpak and install it for the current user
flatpak: flatpak-sources
    flatpak-builder --user --install --force-clean --install-deps-from=flathub build-dir pkg/flatpak/io.github.lemonxah.ZestBay.yml
