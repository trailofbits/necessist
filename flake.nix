{
  description = "Necessist — find unnecessary code in tests";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          # Locked dependencies require a newer Rust than nixpkgs 26.05 provides.
          toolchain = pkgs.rust-bin.stable.latest.minimal;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
          manifest = builtins.fromTOML (builtins.readFile ./necessist/Cargo.toml);
          name = manifest.package.name;
        in
        {
          necessist = rustPlatform.buildRustPackage {
            pname = name;
            version = manifest.package.version;
            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = path: type: builtins.baseNameOf path != "target" && pkgs.lib.cleanSourceFilter path type;
            };

            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [
              "--package"
              name
            ];

            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [
              pkgs.libgit2
              pkgs.openssl
              pkgs.sqlite
            ];

            # Integration tests require network access and external test frameworks.
            doCheck = false;
            doInstallCheck = true;
            installCheckPhase = ''
              runHook preInstallCheck
              "$out/bin/${name}" --version
              runHook postInstallCheck
            '';

            meta = {
              description = manifest.package.description;
              homepage = manifest.package.repository;
              license = pkgs.lib.meta.getLicenseFromSpdxId manifest.package.license;
              mainProgram = name;
              platforms = systems;
            };
          };
          default = self.packages.${system}.necessist;
        }
      );
    };
}
