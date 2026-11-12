# Zestins: ZestBay's own LV2 plugins, built from the zestins/ folder.
{ lib
, stdenv
, pkg-config
, lv2
, libx11
}:

let
  root = ../..;
in
stdenv.mkDerivation {
  pname = "zestins";
  # Ships with ZestBay's version
  version = (lib.importTOML (root + "/Cargo.toml")).package.version;

  src = lib.fileset.toSource {
    root = root + "/zestins";
    fileset = root + "/zestins";
  };

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ lv2 libx11 ];

  makeFlags = [ "LV2_DIR=${placeholder "out"}/lib/lv2" ];

  meta = with lib; {
    description = "LV2 plugins from ZestBay: utility, meter, EQ, gate, compressor, delay, reverb, tone generator, synth and MIDI tools";
    homepage = "https://github.com/lemonxah/zestbay";
    license = licenses.mit;
    platforms = platforms.linux;
  };
}
