# Zestins: ZestBay's own LV2 plugins, built from the zestins/ folder of the
# ZestBay source with the same version.
Name:           zestins
Version:        1.2.7
Release:        1%{?dist}
Summary:        LV2 plugins from ZestBay

License:        MIT
URL:            https://github.com/lemonxah/zestbay
Source0:        zestbay-%{version}.tar.gz

BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  pkg-config
BuildRequires:  lv2-devel
BuildRequires:  libX11-devel

Requires:       libX11
Supplements:    zestbay

%description
Zestins are the LV2 plugins that come with ZestBay: utility (gain, balance,
mute, polarity, mono), meter, EQ, gate, compressor, delay, reverb, tone
generator, a simple synth, a MIDI switch and a MIDI utility. Each has an X11
UI and works in any LV2 host.

%prep
%autosetup -n zestbay-%{version}

%build
make -C zestins CFLAGS="%{optflags}" LDFLAGS="%{build_ldflags}"

%install
make -C zestins install DESTDIR=%{buildroot} LV2_DIR=%{_libdir}/lv2

%files
%license LICENSE
%{_libdir}/lv2/zestins.lv2/

%changelog
* Fri Nov 13 2026 Ryno Kotze <lemon.xah@gmail.com> - 1.2.6-1
- First packaged release of Zestins
