# Zestins

ZestBay's own LV2 plugins: small, dependency-free effects, sources and MIDI
tools that ship with ZestBay and also load in any other LV2 host.

| Plugin | URI | What it does |
|---|---|---|
| Utility | `urn:zestins:utility` | Gain, balance, mute, polarity flip, mono sum |
| Meter | `urn:zestins:meter` | Peak and RMS meters (audio passes through) |
| EQ | `urn:zestins:eq` | Low cut, low shelf, mid peak, high shelf |
| Gate | `urn:zestins:gate` | Noise gate with attack, hold, release, range |
| Compressor | `urn:zestins:compressor` | Stereo-linked, soft knee, gain reduction meter |
| Delay | `urn:zestins:delay` | Stereo echo up to 2 s, tone, ping-pong |
| Reverb | `urn:zestins:reverb` | Stereo room reverb with size, damping, width, freeze |
| Tone Generator | `urn:zestins:tone` | Sine, square, saw, white and pink noise |
| Synth | `urn:zestins:synth` | 16-voice synth with ADSR, sustain pedal, pitch bend |
| MIDI Switch | `urn:zestins:midi-switch` | All MIDI to one of four outputs |
| MIDI Utility | `urn:zestins:midi-utility` | Channel filter/remap, transpose, velocity |

## Layout

- `spec.h`, `spec.c`: every plugin's ports (names, ranges, units, flags). The
  single source of truth.
- `dsp/`: one file per plugin; `zestins.c` lists their descriptors.
- `ui/ui.c`: one X11 UI for all plugins, laid out from the specs.
- `ttlgen.c`: writes the bundle's `.ttl` files from the specs at build time.

Adding a plugin: add its port enum to `spec.h`, its ports and entry to
`spec.c`, a `dsp/<name>.c`, its descriptor to `zestins.c`, and a test in
`src/lv2/tests.rs`.

## Installing

Zestins is packaged separately from ZestBay, with the same version:

| Where | How |
|---|---|
| Ubuntu (PPA `ppa:lemonxah/tools`) | `sudo apt install zestins` |
| Debian 12 / 13 | `sudo apt install ./zestins_<version>-debian13_amd64.deb` from the release page |
| Fedora | `sudo dnf install ./zestins-<version>-1.fc<NN>.x86_64.rpm` from the release page |
| Arch Linux | `makepkg -si` in `pkg/zestins/` (or `pkg/zestins-git/`) |
| Any distro | `make install` below |

The packages install the bundle to `/usr/lib/lv2/zestins.lv2` (`%{_libdir}/lv2`
on Fedora). They are built by `pkg/deb/build-zestins-deb.sh`,
`pkg/rpm/build-zestins-rpm.sh` (`just zestins-deb`, `just zestins-rpm`), the
release workflows, and for the PPA as a second package of the `zestbay`
source package.

ZestBay's level meters (canvas menu → Add meter) are not part of Zestins:
they are built into ZestBay.

## Building

Needs a C compiler, make, pkg-config and the LV2 and X11 headers.

```sh
make                 # build/zestins.lv2
make install         # into ~/.lv2 (ZestBay picks it up on the next plugin scan)
make install DESTDIR="$pkgdir" LV2_DIR=/usr/lib/lv2   # packaging
make uninstall
```

`cargo test` builds the bundle into `target/zestins-test` and runs every
plugin through ZestBay's LV2 host.
