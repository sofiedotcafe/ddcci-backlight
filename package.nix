{
  stdenv,
  lib,
  rustc,
  linuxPackages_latest,
  kernel ? linuxPackages_latest,
  ...
}:

stdenv.mkDerivation {
  pname = "ddci-backlight";
  version = "0.1.0";

  src = ./.;

  nativeBuildInputs = kernel.kernel.moduleBuildDependencies ++ [
    rustc
  ];

  buildInputs = [ kernel.kernel.dev ];

  makeFlags = [
    "KDIR=${kernel.kernel.dev}/lib/modules/${kernel.kernel.modDirVersion}/build"
  ];

  installPhase = ''
    install -Dm444 ddcci_backlight.ko \
      $out/lib/modules/${kernel.kernel.modDirVersion}/extra/ddcci_backlight.ko
  '';

  meta = with lib; {
    description = "Linux kernel module for DDC/CI backlight control written in Rust";
    license = licenses.gpl2Only;
    platforms = platforms.linux;
  };
}
