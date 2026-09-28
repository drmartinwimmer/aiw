{
  description = "Rust development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };
        aiwPackage = rustPlatform.buildRustPackage {
          pname = "aiw";
          version = "0.1.1";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeCheckInputs = [
            pkgs.git
            pkgs.jujutsu
            pkgs.fence
            pkgs.bubblewrap
          ];
          preCheck = ''
            export HOME=$(mktemp -d)
            mkdir -p $HOME/bin
            cat << 'EOF' > $HOME/bin/agy
            #!/bin/sh
            echo "agy 1.1.24"
            EOF
            chmod +x $HOME/bin/agy
            export PATH="$HOME/bin:$PATH"
          '';
          postInstall = ''
            install -Dm644 templates/fence.jsonc $out/share/aiw/fence.jsonc
          '';
        };
      in
      {
        # Development shell
        devShells.default =
          with pkgs;
          mkShell {
            buildInputs = [
              toolchain
              (cargo-audit.override { inherit rustPlatform; })
            ];
          };

        packages = {
          default = aiwPackage;
          aiw = aiwPackage;
        };

        apps = {
          default = flake-utils.lib.mkApp {
            drv = aiwPackage;
          };
          aiw = flake-utils.lib.mkApp {
            drv = aiwPackage;
          };
        };
      }
    );
}
