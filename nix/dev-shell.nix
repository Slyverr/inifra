{ pkgs, rustToolchain }:

pkgs.mkShell {
  AWS_LC_SYS_USE_SYSTEM = "0";

  packages = [
    pkgs.cmake
    rustToolchain
    pkgs.git
    pkgs.openssh
    pkgs.perl
  ];
}
