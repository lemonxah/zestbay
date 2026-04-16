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

# Build release binary
build:
    cargo build --workspace --release

# Build and install locally
install: build
    sudo cp target/release/zestbay /usr/bin/zestbay
    sudo cp target/release/zestbay-ui-bridge /usr/lib/zestbay/zestbay-ui-bridge
    @echo "Installed"

aur_dir := "../aur/zestbay"

# Bump, commit, tag, and push. Optional message: just release 0.5.0 "fix remapping"
release new_version msg="":
    just bump {{new_version}}
    git add Cargo.toml Cargo.lock pkg/zestbay/PKGBUILD pkg/rpm/zestbay.spec
    git commit -m "v{{new_version}}{{ if msg != "" { ": " + msg } else { "" } }}"
    git tag -a "v{{new_version}}" -m "v{{new_version}}{{ if msg != "" { ": " + msg } else { "" } }}"
    git push && git push --tags
    gh release create "v{{new_version}}" --generate-notes
    @just aur-publish {{new_version}}

# Update AUR repo with new version
aur-publish new_version:
    sed -i 's/^pkgver=.*/pkgver={{new_version}}/' {{aur_dir}}/PKGBUILD
    cd {{aur_dir}} && makepkg --printsrcinfo > .SRCINFO
    cd {{aur_dir}} && git commit -am "updated the version to {{new_version}}" && git push
