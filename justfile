version := `grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'`

[doc('Show current version')]
current:
    @echo "{{version}}"

[doc('Bump version: just bump 0.3.0')]
bump new_version:
    sed -i '0,/^version = ".*"/s//version = "{{new_version}}"/' Cargo.toml
    cargo update --workspace
    sed -i 's/^pkgver=.*/pkgver={{new_version}}/' pkg/zestbay/PKGBUILD pkg/zestins/PKGBUILD
    sed -i 's/^Version:.*/Version:        {{new_version}}/' pkg/rpm/zestbay.spec pkg/rpm/zestins.spec
    @echo "Bumped to {{new_version}}"

[doc('Run all tests')]
test:
    cargo test --workspace

[doc('Build release binary')]
build:
    cargo build --workspace --release

[doc('Build and install locally')]
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

[doc('Bump, commit, tag, and push. Optional message: just release 0.5.0 "fix remapping"')]
release new_version msg="":
    just bump {{new_version}}
    git add Cargo.toml Cargo.lock pkg/zestbay/PKGBUILD pkg/zestins/PKGBUILD pkg/rpm/zestbay.spec pkg/rpm/zestins.spec
    if git diff --cached --quiet; then \
        echo "Version bump already committed; continuing release"; \
    else \
        git commit -m "v{{new_version}}{{ if msg != "" { ": " + msg } else { "" } }}"; \
    fi
    git tag -a "v{{new_version}}" -m "v{{new_version}}{{ if msg != "" { ": " + msg } else { "" } }}"
    git push && git push --tags
    gh release create "v{{new_version}}" --generate-notes

[doc('Build a .deb inside an Ubuntu container (podman/docker): pkg/deb/zestbay_<version>-<image>_amd64.deb')]
deb image="ubuntu:24.04":
    pkg/deb/build-deb.sh --container {{image}}

[doc('Build zestins/build/zestins.lv2')]
zestins:
    make -C zestins

[doc('Install the Zestins bundle into ~/.lv2 (for trying them without a package)')]
zestins-install:
    make -C zestins install

[doc('Remove the Zestins bundle from ~/.lv2')]
zestins-uninstall:
    make -C zestins uninstall

[doc('Build the Zestins .deb inside an Ubuntu container: pkg/deb/zestins_<version>-<image>_amd64.deb')]
zestins-deb image="ubuntu:24.04":
    pkg/deb/build-zestins-deb.sh --container {{image}}

[doc('Build the Zestins .rpm inside a Fedora container: pkg/rpm/rpmbuild/RPMS/')]
zestins-rpm image="fedora:latest":
    pkg/rpm/build-zestins-rpm.sh --container {{image}}
