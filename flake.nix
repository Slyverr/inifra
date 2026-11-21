{
  description = "Inifra project bootstrapper";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      eachSystem = nixpkgs.lib.genAttrs systems;

      forSystem = system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.stable."1.100.0".minimal.override {
            extensions = [ "clippy" "rustfmt" ];
          };

          inifra = import ./nix/package.nix {
            inherit pkgs rustToolchain;
            src = self;
          };
        in
        {
          inherit inifra pkgs rustToolchain;
        };
    in
    {
      packages = eachSystem (system:
        let package = (forSystem system).inifra;
        in {
          default = package;
          inifra = package;
        });

      apps = eachSystem (system:
        let
          package = (forSystem system).inifra;
          app = {
            type = "app";
            program = "${package}/bin/inifra";
            meta.description = "Inifra project bootstrapper";
          };
        in {
          default = app;
          inifra = app;
        });

      checks = eachSystem (system: {
        package = (forSystem system).inifra;
      });

      devShells = eachSystem (system:
        let
          environment = forSystem system;
        in
        {
          default = import ./nix/dev-shell.nix {
            inherit (environment) pkgs rustToolchain;
          };
        });
    };
}
