{ pkgs, rustToolchain, src }:

let
  inherit (pkgs) lib;

  rustPlatform = pkgs.makeRustPlatform {
    cargo = rustToolchain;
    rustc = rustToolchain;
  };

  workspace = builtins.fromTOML (builtins.readFile "${src}/Cargo.toml");
in
rustPlatform.buildRustPackage {
  pname = "inifra";
  version = workspace.workspace.package.version;

  inherit src;

  cargoLock.lockFile = "${src}/Cargo.lock";
  cargoBuildFlags = [ "--package" "inifra" ];
  cargoTestFlags = [ "--workspace" ];
  dontCargoUseSystemLibs = true;
  AWS_LC_SYS_USE_SYSTEM = "0";
  SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";

  nativeBuildInputs = [
    pkgs.cmake
    pkgs.git
    pkgs.makeWrapper
    pkgs.openssh
    pkgs.perl
  ];

  postInstall = ''
    wrapProgram "$out/bin/inifra" \
      --prefix PATH : "${lib.makeBinPath [ pkgs.git pkgs.openssh ]}" \
      --set-default SSL_CERT_FILE "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"

    install -Dm644 ${src}/LICENSE-APACHE "$out/share/licenses/inifra/LICENSE-APACHE"
    install -Dm644 ${src}/LICENSE-MIT "$out/share/licenses/inifra/LICENSE-MIT"
  '';

  meta = {
    description = "A CLI for bootstrapping projects with reusable file templates";
    homepage = "https://github.com/Slyverr/inifra";
    license = with lib.licenses; [ asl20 mit ];
    mainProgram = "inifra";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
    ];
  };
}
