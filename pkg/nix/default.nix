{
  lib,
  rustPlatform,
  fetchFromGitHub,
  pkg-config,
  cmake,
  clang,
  pipewire,
  qt6,
  lilv,
  lv2,
  xorg,
  dbus,
  copyDesktopItems,
  makeDesktopItem,
}:

rustPlatform.buildRustPackage rec {
  pname = "zestbay";
  version = "0.7.0";

  src = fetchFromGitHub {
    owner = "lemonxah";
    repo = "zestbay";
    rev = "v${version}";
    hash = lib.fakeHash; # Replace with real hash after first build
  };

  cargoHash = lib.fakeHash; # Replace with real hash after first build

  # Build the entire workspace (main binary + UI bridge)
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
    lv2
    xorg.libX11
    dbus
  ];

  # CXX-Qt needs to find Qt tools
  env = {
    LIBCLANG_PATH = "${clang.cc.lib}/lib";
  };

  postInstall = ''
    # Install UI bridge binary to lib directory
    install -Dm755 target/release/zestbay-ui-bridge $out/lib/zestbay/zestbay-ui-bridge

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
