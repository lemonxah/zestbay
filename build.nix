{
  lib,
  rustPlatform,
  nix-gitignore,
  dbus,
  pkg-config,
  qt6,
  pipewire,
  lilv,
  lv2,
  serd,
  suil,
  gtk3,
  cmake,
  makeWrapper,
  openssl,
}: let
  version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version + "-git";
  src = nix-gitignore.gitignoreSource [] ./.;

  #   This and the following hacks to make qmake find qmlcachegen correctly are from
  #   https://github.com/NixOS/nixpkgs/issues/486645
  #   https://discourse.nixos.org/t/python-qt-woes/11808/10
  #   , this could be helpfull in case the qmake situation won't get better
  qtDeps = with qt6; [
    qtbase
    qtdeclarative
  ];
  qtEnv = with qt6; env "qt-custom-${qtbase.version}" qtDeps;
in
  rustPlatform.buildRustPackage {
    pname = "zestbay";
    inherit version src;

    cargoLock.lockFile = ./Cargo.lock;

    nativeBuildInputs = [
      pkg-config
      qtEnv
      cmake
      rustPlatform.bindgenHook
      qt6.wrapQtAppsHook
      qt6.qmake
      makeWrapper
      openssl
    ];
    buildInputs =
      [
        lilv
        lv2
        serd
        suil
        gtk3
        pipewire
        dbus
      ]
      ++ qtDeps;

    cargoBuildFlags = ["--workspace"];
    enableParallelBuilding = true;

    preBuild = ''
      # Add Qt-related environment variables.
      # https://discourse.nixos.org/t/python-qt-woes/11808/10
      setQtEnvironment=$(mktemp)
      random=$(openssl rand -base64 20 | sed "s/[^a-zA-Z0-9]//g")
      makeWrapper "$(type -p sh)" "$setQtEnvironment" "''${qtWrapperArgs[@]}" --argv0 "$random"
      sed "/$random/d" -i "$setQtEnvironment"
      source "$setQtEnvironment"
      export QMAKE="${qtEnv}/bin/qmake"
    '';

    postInstall = ''
      install -Dm444 zestbay.desktop $out/share/applications/zestbay.desktop
      install -Dm644 images/zesticon.png $out/share/icons/hicolor/256x256/apps/zestbay.png
      install -Dm644 images/zesttray.png $out/share/icons/hicolor/256x256/apps/zestbay-tray.png
    '';

    meta = {
      description = "A PipeWire patchbay for Linux that visualizes your audio graph, hosts LV2 effects plugins inline, and auto-connects ports with persistent routing rules.";
      license = lib.licenses.mit;
      mainProgram = "zestbay";
      homepage = "https://github.com/lemonxah/zestbay";
      platforms = lib.platforms.linux;
    };
  }
