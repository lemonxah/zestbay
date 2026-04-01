Name:           zestbay
Version:        0.7.0
Release:        1%{?dist}
Summary:        PipeWire patchbay and audio routing manager

License:        MIT
URL:            https://github.com/lemonxah/zestbay
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.85
BuildRequires:  cargo
BuildRequires:  clang
BuildRequires:  cmake
BuildRequires:  pkg-config
BuildRequires:  pipewire-devel
BuildRequires:  qt6-qtbase-devel
BuildRequires:  qt6-qtdeclarative-devel
BuildRequires:  lilv-devel
BuildRequires:  lv2-devel
BuildRequires:  libX11-devel
BuildRequires:  dbus-devel

Requires:       pipewire
Requires:       qt6-qtbase
Requires:       qt6-qtdeclarative
Requires:       lilv
Requires:       libX11
Requires:       dbus

%description
ZestBay is a PipeWire patchbay application with LV2, CLAP, and VST3
plugin hosting, MIDI mapping, and a visual node-graph editor for
routing audio and MIDI between applications and devices.

%prep
%autosetup

%build
export RUSTUP_TOOLCHAIN=stable
export CARGO_TARGET_DIR=target
cargo build --workspace --release

%install
install -Dm755 target/release/%{name} %{buildroot}%{_bindir}/%{name}
install -Dm755 target/release/zestbay-ui-bridge %{buildroot}%{_libdir}/%{name}/zestbay-ui-bridge
install -Dm644 zestbay.desktop %{buildroot}%{_datadir}/applications/zestbay.desktop
install -Dm644 images/zesticon.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/zestbay.png
install -Dm644 images/zesttray.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/zestbay-tray.png
install -Dm644 LICENSE %{buildroot}%{_datadir}/licenses/%{name}/LICENSE

%files
%license LICENSE
%{_bindir}/%{name}
%{_libdir}/%{name}/zestbay-ui-bridge
%{_datadir}/applications/zestbay.desktop
%{_datadir}/icons/hicolor/256x256/apps/zestbay.png
%{_datadir}/icons/hicolor/256x256/apps/zestbay-tray.png

%changelog
* Tue Apr 01 2026 Ryno Kotze <lemon.xah@gmail.com> - 0.7.0-1
- MIDI input support for LV2, CLAP, and VST3 plugins
- Media type validation for port connections
- Fix plugin processing when output ports are unconnected
