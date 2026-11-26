{
  lib,
  stdenv,
  cmake,
  pkg-config,
  cargo,
  rustc,
  rustPlatform,
  qt6,
  kdePackages,
  pipewire,
  src,
  version,
}:
stdenv.mkDerivation {
  pname = "drink-kde";
  inherit src version;

  cargoDeps = rustPlatform.importCargoLock {
    lockFile = ../Cargo.lock;
  };
  nativeBuildInputs = [
    cargo
    rustc
    rustPlatform.cargoSetupHook
    cmake
    pkg-config
    qt6.wrapQtAppsHook
  ];
  buildInputs = [
    qt6.qtbase
    qt6.qtwayland
    qt6.qtsvg
    kdePackages.layer-shell-qt
    kdePackages.kservice
    kdePackages.kstatusnotifieritem
    pipewire
  ];
  cmakeDir = "../linux";
  cmakeFlags = [
    (lib.cmakeFeature "DRINK_RELEASE_VERSION" version)
  ];

  doCheck = true;
  checkPhase = ''
    runHook preCheck
    ctest --output-on-failure -LE plasma
    runHook postCheck
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    test "$("$out/bin/drink-kde" --version)" = "Drink ${version}"
    test -f "$out/share/applications/io.github.napolitain.Drink.desktop"
    test -f "$out/share/icons/hicolor/scalable/apps/io.github.napolitain.Drink.svg"
    runHook postInstallCheck
  '';

  meta = {
    description = "Water-break reminders for KDE Plasma Wayland";
    homepage = "https://github.com/Napolitain/drink";
    mainProgram = "drink-kde";
    platforms = [ "x86_64-linux" ];
  };
}
