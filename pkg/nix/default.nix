{
  lib,
  rustPlatform,
  pkg-config,
  cmake,
  clang,
  pipewire,
  qt6,
  lilv,
  serd,
  lv2,
  suil,
  gtk3,
  libx11,
  dbus,
  ladspa-header,
  copyDesktopItems,
  makeDesktopItem,
}:

let
  root = ../..;
  qtEnv = qt6.env "zestbay-qt-${qt6.qtbase.version}" [ qt6.qtdeclarative ];
in
rustPlatform.buildRustPackage {
  pname = "zestbay";
  version = (lib.importTOML (root + "/Cargo.toml")).package.version;

  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      (root + "/Cargo.toml")
      (root + "/Cargo.lock")
      (root + "/build.rs")
      (root + "/src")
      (root + "/crates")
      (root + "/qml")
      (root + "/fonts")
      (root + "/images")
      (root + "/LICENSE")
      (root + "/zestins")
    ];
  };

  cargoLock.lockFile = root + "/Cargo.lock";

  cargoBuildFlags = [ "--workspace" ];

  nativeBuildInputs = [
    pkg-config
    cmake
    clang
    qt6.wrapQtAppsHook
    copyDesktopItems
  ];

  buildInputs = [
    pipewire
    qt6.qtbase
    qt6.qtdeclarative
    lilv
    serd
    lv2
    suil
    gtk3
    libx11
    dbus
  ];

  checkInputs = [ ladspa-header ];

  env = {
    LIBCLANG_PATH = "${clang.cc.lib}/lib";
  };

  preBuild = ''
    export QMAKE=${qtEnv}/bin/qmake
  '';

  postInstall = ''
    install -Dm755 $out/bin/zestbay-ui-bridge $out/lib/zestbay/zestbay-ui-bridge
    rm $out/bin/zestbay-ui-bridge

    install -Dm644 images/logo/zestbay-launcher.svg $out/share/icons/hicolor/scalable/apps/zestbay.svg
    install -Dm644 images/zesticon.png $out/share/icons/hicolor/256x256/apps/zestbay.png
    install -Dm644 images/zesttray.png $out/share/icons/hicolor/256x256/apps/zestbay-tray.png
  '';

  desktopItems = [
    (makeDesktopItem {
      name = "zestbay";
      desktopName = "ZestBay";
      comment = "PipeWire patchbay and audio routing manager";
      exec = "zestbay";
      icon = "zestbay";
      categories = [ "AudioVideo" "Audio" "Mixer" ];
    })
  ];

  meta = with lib; {
    description = "PipeWire patchbay and audio routing manager with LV2/CLAP/VST3 plugin hosting";
    homepage = "https://github.com/lemonxah/zestbay";
    license = licenses.mit;
    maintainers = [ ];
    platforms = platforms.linux;
    mainProgram = "zestbay";
  };
}
